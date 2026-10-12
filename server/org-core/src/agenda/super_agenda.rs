//! org-super-agenda (1.4-pre, as of fb20ad9): the lines of an agenda view
//! in groups, each under a header, in place of one list. Emacs runs it
//! on what `org-agenda-finalize-entries` returns, so the lines of each
//! day of the agenda, and of each other view, are grouped on their own.
//!
//! A group is a list of selectors and their arguments, as Emacs's plist
//! `(:name "Today" :time-grid t)` is: an object of them in their order
//! (`{ "name": "Today", "time-grid": true }`), or a list of names and
//! arguments in turn. `true` stands for `t`, `false` or `null` for `nil`,
//! and the words a selector takes (`past`, `today`, `todo` …) are
//! strings; `"name": false` is Emacs's `none`, a group without a header.
//! The selectors that call Emacs Lisp (`:pred`, `:auto-map`,
//! `:transformer`, a function for `:property`) are refused: there is no
//! Lisp to run. `:face` is left out: the lines keep their own faces.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use regex::Regex;
use serde_json::Value;

use super::item::{Item, ItemKind};
use super::matcher::emacs_regex;
use super::Config;
use crate::date::{DateTime, Day};
use crate::outline::{complex_heading_parts, complex_heading_title, Entry, Outline};
use crate::timestr::time_string_to_day;

/// What a header is put after (`org-super-agenda-header-separator`,
/// `org-super-agenda-final-group-separator`): text, or a character
/// repeated across the window, then a newline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Separator {
    Text(String),
    Char(char),
}

/// The options of org-super-agenda, and its groups.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// `org-super-agenda-groups`
    pub groups: Vec<Value>,
    /// `org-super-agenda-unmatched-name`
    pub unmatched_name: String,
    /// `org-super-agenda-unmatched-order`
    pub unmatched_order: i64,
    /// `org-super-agenda-header-separator`
    pub header_separator: Separator,
    /// `org-super-agenda-header-prefix`
    pub header_prefix: String,
    /// `org-super-agenda-final-group-separator`
    pub final_group_separator: Separator,
    /// `org-super-agenda-date-format`
    pub date_format: String,
    /// `org-super-agenda-hide-empty-groups`
    pub hide_empty_groups: bool,
    /// `org-super-agenda-keep-order`
    pub keep_order: bool,
    /// `org-super-agenda-group-property-name`
    pub group_property_name: String,
    /// `org-super-agenda-properties-inherit`
    pub properties_inherit: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            groups: Vec::new(),
            unmatched_name: "Other items".to_string(),
            unmatched_order: 99,
            header_separator: Separator::Text("\n".to_string()),
            header_prefix: " ".to_string(),
            final_group_separator: Separator::Text(String::new()),
            date_format: "%e %B %Y".to_string(),
            hide_empty_groups: false,
            keep_order: false,
            group_property_name: "agenda-group".to_string(),
            properties_inherit: true,
        }
    }
}

/// A line of the grouped view: a line of a header, with where the part
/// in the header's face starts (the separator before it is not), or an
/// item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Grouped {
    Header { text: String, faced: usize },
    Item(Item),
}

/// The selectors that only say how a group shows
/// (`org-super-agenda-special-selectors`).
const SPECIAL: &[&str] = &["name", "order", "face", "transformer"];

/// The selectors of a group, in their order, without their colons.
fn selectors(group: &Value) -> Result<Vec<(String, Value)>, String> {
    let key = |name: &str| name.trim_start_matches(':').to_string();
    match group {
        Value::Object(map) => Ok(map.iter().map(|(name, value)| (key(name), value.clone())).collect()),
        Value::Array(list) => Ok(list
            .chunks(2)
            .filter_map(|pair| Some((key(pair[0].as_str()?), pair.get(1).cloned().unwrap_or(Value::Null))))
            .collect()),
        Value::Null | Value::Bool(false) => Ok(Vec::new()),
        other => Err(format!("Invalid org-super-agenda-groups group: {other}")),
    }
}

/// A selector's argument as the list Emacs makes of it: one that is not a
/// list is put in one, `nil` is the empty one.
fn arguments(value: &Value) -> Vec<Value> {
    match value {
        Value::Array(list) => list.clone(),
        Value::Null | Value::Bool(false) => Vec::new(),
        other => vec![other.clone()],
    }
}

/// A list of arguments as strings, for selectors that take strings.
fn strings(args: &[Value]) -> Vec<String> {
    args.iter().map(|value| value.as_str().map_or_else(|| lisp(value), str::to_string)).collect()
}

/// How Emacs prints a value, for a message or a name.
fn lisp(value: &Value) -> String {
    match value {
        Value::Null | Value::Bool(false) => "nil".to_string(),
        Value::Bool(true) => "t".to_string(),
        Value::String(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        Value::Array(list) => format!("({})", list.iter().map(lisp).collect::<Vec<_>>().join(" ")),
        Value::Object(_) => value.to_string(),
    }
}

/// The first argument as Emacs's `pcase` sees it: `t`, `nil`, a symbol or
/// a string.
fn first(args: &[Value]) -> Option<&Value> {
    args.first()
}

fn is_t(value: Option<&Value>) -> bool {
    matches!(value, Some(Value::Bool(true)))
}

fn is_nil(value: Option<&Value>) -> bool {
    matches!(value, None | Some(Value::Null) | Some(Value::Bool(false)))
}

/// What a selector found.
enum Matched {
    /// The items it selected.
    Items(Vec<usize>),
    /// Groups of items, each with its header (an auto selector).
    Groups(Vec<(String, Vec<usize>)>),
}

/// What a selector gives back: the name of its section (`Some` of a
/// string), or the auto selector it is; the items it left; what it found.
struct Outcome {
    name: Name,
    rest: Vec<usize>,
    found: Matched,
}

#[derive(Clone)]
enum Name {
    None,
    Text(String),
    Auto(String),
}

/// One section of the grouped list.
struct Section {
    name: Option<String>,
    items: Vec<usize>,
    order: i64,
}

/// The facts of an agenda line that the selectors look at.
struct Facts<'a> {
    item: &'a Item,
    outline: Option<&'a Outline>,
    entry: Option<(usize, &'a Entry)>,
    path: Option<&'a str>,
}

/// The grouping of one list of lines.
pub struct Grouper<'a> {
    files: &'a [&'a Outline],
    paths: &'a [String],
    settings: &'a Settings,
    config: &'a Config,
    today: Day,
    /// How the view sorts (`org-entries-lessp`): by time first in the
    /// agenda, by priority in the other lists, not at all in a search,
    /// whose only strategy, `category-keep`, keeps the order.
    sorting: Option<bool>,
}

impl<'a> Grouper<'a> {
    pub fn new(
        files: &'a [&'a Outline],
        config: &'a Config,
        settings: &'a Settings,
        today: Day,
        sorting: Option<bool>,
    ) -> Self {
        Grouper { files, paths: &config.file_paths, settings, config, today, sorting }
    }

    fn facts<'b>(&'b self, items: &'b [Item], index: usize) -> Facts<'b> {
        let item = &items[index];
        let outline = item.source.and_then(|source| self.files.get(source.file).copied());
        let entry = item.source.and_then(|source| Some((source.entry, outline?.entries.get(source.entry)?)));
        let path = item.source.and_then(|source| self.paths.get(source.file)).map(String::as_str);
        Facts { item, outline, entry, path }
    }

    /// `org-super-agenda--group-items`: `items`, sorted, in their groups.
    pub fn group(&self, items: Vec<Item>) -> Result<Vec<Grouped>, String> {
        // Blank lines go, as `s-blank-str?` strings are removed.
        let blank = |item: &Item| item.text.trim().is_empty();
        if self.settings.groups.is_empty() || items.is_empty() {
            return Ok(items.into_iter().filter(|item| !blank(item)).map(Grouped::Item).collect());
        }
        let groups = transform(&self.settings.groups)?;
        let mut all: Vec<usize> = (0..items.len()).collect();
        let mut sections = Vec::new();
        for group in &groups {
            let selectors = selectors(group)?;
            let custom = selectors.iter().find(|(key, _)| key == "name").map(|(_, value)| value.clone());
            let order =
                selectors.iter().find(|(key, _)| key == "order").and_then(|(_, value)| value.as_i64()).unwrap_or(0);
            for (key, _) in &selectors {
                if matches!(key.as_str(), "transformer") {
                    return Err(unsupported(key));
                }
            }
            let outcome = self.dispatch(&items, &all, &selectors)?;
            all = outcome.rest;
            match (outcome.name, outcome.found) {
                (Name::Auto(_), Matched::Groups(groups)) => {
                    for (name, mut members) in groups {
                        self.keep_order(&items, &mut members);
                        sections.push(Section { name: Some(name), items: members, order });
                    }
                }
                (name, found) => {
                    let mut members = match found {
                        Matched::Items(members) => members,
                        Matched::Groups(groups) => groups.into_iter().flat_map(|(_, members)| members).collect(),
                    };
                    self.keep_order(&items, &mut members);
                    let name = match custom {
                        // `none`: no header.
                        Some(Value::Bool(false)) => None,
                        Some(Value::String(text)) => Some(text),
                        _ => match name {
                            Name::Text(text) => Some(text),
                            Name::None | Name::Auto(_) => None,
                        },
                    };
                    sections.push(Section { name, items: members, order });
                }
            }
        }
        sections.insert(
            0,
            Section {
                name: Some(self.settings.unmatched_name.clone()),
                items: all,
                order: self.settings.unmatched_order,
            },
        );
        // Lowest order first; sections of the same order that is not 0 by
        // name. Emacs's `sort` is stable.
        sections.sort_by(|a, b| {
            let less = |a: &Section, b: &Section| match (&a.name, &b.name) {
                (Some(x), Some(y)) if a.order == b.order && a.order != 0 => x < y,
                _ => a.order < b.order,
            };
            if less(a, b) {
                std::cmp::Ordering::Less
            } else if less(b, a) {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        });
        let mut slots: Vec<Option<Item>> = items.into_iter().map(Some).collect();
        let mut out = Vec::new();
        for section in sections {
            if section.items.is_empty() {
                continue;
            }
            if let Some(name) = &section.name {
                out.extend(self.header(name));
            }
            for index in section.items {
                if let Some(item) = slots[index].take().filter(|item| !blank(item)) {
                    out.push(Grouped::Item(item));
                }
            }
        }
        self.final_separator(&mut out);
        Ok(out)
    }

    /// `org-super-agenda-keep-order`: the items of a group in the order
    /// the view sorts them in (`org-entries-lessp`).
    fn keep_order(&self, items: &[Item], members: &mut [usize]) {
        if self.settings.keep_order {
            if let Some(by_time) = self.sorting {
                members.sort_by(|a, b| super::compare(&items[*a], &items[*b], by_time));
            }
        }
    }

    /// `org-super-agenda--make-agenda-header`: the lines of a header, the
    /// prefix and the name in the header's face.
    fn header(&self, name: &str) -> Vec<Grouped> {
        let separator = match &self.settings.header_separator {
            Separator::Char(c) => format!("{}\n", c.to_string().repeat(self.width())),
            Separator::Text(text) => text.clone(),
        };
        let text = format!("{separator}{}{name}", self.settings.header_prefix);
        // A blank header goes: the list is joined after `s-blank-str?`
        // strings are removed, and a header is one of its strings.
        if text.trim().is_empty() {
            return Vec::new();
        }
        let mut lines: Vec<&str> = text.split('\n').collect();
        let last = lines.pop().unwrap_or("");
        let mut out: Vec<Grouped> =
            lines.iter().map(|line| Grouped::Header { text: line.to_string(), faced: line.len() }).collect();
        // The separator's own text on the name's line is not in the face.
        let faced = separator.rsplit('\n').next().unwrap_or("").len();
        out.push(Grouped::Header { text: last.to_string(), faced });
        out
    }

    /// The width of the agenda's window, as `window-width` gives it.
    fn width(&self) -> usize {
        self.config.window_width.unwrap_or(80)
    }

    /// `org-super-agenda-final-group-separator`, put after the last line.
    fn final_separator(&self, out: &mut Vec<Grouped>) {
        let text = match &self.settings.final_group_separator {
            Separator::Char(c) => format!("\n{}", c.to_string().repeat(self.width())),
            Separator::Text(text) => text.clone(),
        };
        // Every item discarded: the view still gets the separator, an
        // empty line when that is empty.
        if out.is_empty() {
            out.extend(text.split('\n').map(|part| Grouped::Header { text: part.to_string(), faced: part.len() }));
            return;
        }
        if text.is_empty() {
            return;
        }
        // Put at the end of the last line before its tags are aligned,
        // which then are not: they are not at the end of the line.
        let mut parts = text.split('\n');
        let head = parts.next().unwrap_or("");
        match out.last_mut() {
            Some(Grouped::Item(item)) => item.text.push_str(head),
            Some(Grouped::Header { text, .. }) => text.push_str(head),
            None => {}
        }
        out.extend(parts.map(|part| Grouped::Header { text: part.to_string(), faced: part.len() }));
    }

    /// `org-super-agenda--group-dispatch`: the items any selector of
    /// `selectors` selects, each taking from what the ones before left.
    fn dispatch(&self, items: &[Item], from: &[usize], selectors: &[(String, Value)]) -> Result<Outcome, String> {
        let mut rest = from.to_vec();
        let mut found_items = Vec::new();
        let mut found_groups = Vec::new();
        let mut names: Vec<Name> = Vec::new();
        for (key, args) in selectors {
            if SPECIAL.contains(&key.as_str()) {
                continue;
            }
            let outcome = self.select(items, &rest, key, args)?;
            match outcome.found {
                Matched::Items(found) => found_items.extend(found),
                Matched::Groups(groups) => found_groups.extend(groups),
            }
            names.push(outcome.name);
            rest = outcome.rest;
        }
        let name = match names.first() {
            Some(Name::Text(_)) => Name::Text(
                names
                    .iter()
                    .filter_map(|name| match name {
                        Name::Text(text) => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join(" and "),
            ),
            Some(other) => other.clone(),
            None => Name::None,
        };
        let found = match name {
            Name::Auto(_) => Matched::Groups(found_groups),
            _ => Matched::Items(found_items),
        };
        Ok(Outcome { name, rest, found })
    }

    /// The items of `from` that `test` holds for, and the others; under
    /// `name`.
    fn split(&self, items: &[Item], from: &[usize], name: Name, test: impl Fn(&Facts) -> bool) -> Outcome {
        let (found, rest): (Vec<usize>, Vec<usize>) = from.iter().partition(|index| test(&self.facts(items, **index)));
        Outcome { name, rest, found: Matched::Items(found) }
    }

    /// The items of `from` in groups by the key `key` gives each, the
    /// groups in the order of `sort`, headed by `header` of their key
    /// (`org-super-agenda--def-auto-group`).
    fn auto<K: Clone>(
        &self,
        items: &[Item],
        from: &[usize],
        selector: &str,
        key: impl Fn(&Facts) -> Option<(String, K)>,
        sort: impl Fn(&(String, K), &(String, K)) -> std::cmp::Ordering,
        header: impl Fn(&str) -> String,
    ) -> Outcome {
        let mut groups: Vec<((String, K), Vec<usize>)> = Vec::new();
        let mut rest = Vec::new();
        for index in from {
            match key(&self.facts(items, *index)) {
                Some(found) => match groups.iter_mut().find(|(known, _)| known.0 == found.0) {
                    Some((_, members)) => members.push(*index),
                    None => groups.push((found, vec![*index])),
                },
                None => rest.push(*index),
            }
        }
        groups.sort_by(|a, b| sort(&a.0, &b.0));
        let groups = groups.into_iter().map(|((name, _), members)| (header(&name), members)).collect();
        Outcome { name: Name::Auto(selector.to_string()), rest, found: Matched::Groups(groups) }
    }

    /// The auto groups whose keys are strings, sorted with `string<`.
    fn auto_by_name(
        &self,
        items: &[Item],
        from: &[usize],
        selector: &str,
        key: impl Fn(&Facts) -> Option<String>,
        header: impl Fn(&str) -> String,
    ) -> Outcome {
        self.auto(items, from, selector, |facts| key(facts).map(|name| (name, ())), |a, b| a.0.cmp(&b.0), header)
    }

    /// One selector over the items `from`.
    fn select(&self, items: &[Item], from: &[usize], key: &str, value: &Value) -> Result<Outcome, String> {
        let args = arguments(value);
        let text = |name: String| Name::Text(name);
        Ok(match key {
            "date" => {
                let arg = first(&args);
                let which = if is_t(arg) {
                    0
                } else if is_nil(arg) {
                    1
                } else if arg.and_then(Value::as_str) == Some("today") {
                    2
                } else {
                    return Err("Argument to `:date' must be `t', `nil', or `today'".to_string());
                };
                let today = self.today;
                self.split(items, from, text("Dated items".to_string()), move |facts| match which {
                    0 => facts.item.ts_date.is_some(),
                    1 => facts.item.ts_date.is_none(),
                    _ => facts.item.ts_date == Some(today),
                })
            }
            "time-grid" => {
                self.split(items, from, text("Timed items".to_string()), |facts| facts.item.time_of_day.is_some())
            }
            "deadline" | "scheduled" => self.planning(items, from, key, &args)?,
            "effort<" | "effort>" => {
                let wanted = strings(&args);
                let limit = wanted.first().and_then(|text| duration_minutes(text)).unwrap_or(0.0);
                let less = key == "effort<";
                let name = format!("Effort {} {} items", &key[6..], wanted.join(" or "));
                self.split(items, from, text(name), move |facts| {
                    // The search view's lines have no `effort`.
                    let entry = facts.entry.filter(|_| facts.item.kind != ItemKind::Search);
                    let effort = entry.and_then(|(_, entry)| super::effort(entry)).and_then(duration_minutes);
                    effort.is_some_and(|minutes| if less { minutes <= limit } else { minutes >= limit })
                })
            }
            "anything" => self.split(items, from, Name::None, |_| true),
            "children" => {
                let arg = first(&args).cloned();
                let name = match &arg {
                    Some(Value::String(word)) if word == "todo" => Some("Items with child to-dos".to_string()),
                    Some(Value::String(word)) => Some(format!("Items with children {word}")),
                    Some(Value::Bool(true)) => Some("Items with children".to_string()),
                    _ => Some("Items without children".to_string()),
                };
                let keywords = strings(&args);
                self.split(items, from, name.map_or(Name::None, Name::Text), move |facts| {
                    let (Some(outline), Some((index, _))) = (facts.outline, facts.entry) else { return false };
                    let descendants = descendants(outline, index);
                    match &arg {
                        Some(Value::String(word)) if word == "todo" => {
                            descendants.iter().any(|child| outline.entries[*child].is_open_todo())
                        }
                        Some(Value::String(_)) => descendants.iter().any(|child| {
                            outline.entries[*child].todo.as_ref().is_some_and(|todo| keywords.contains(todo))
                        }),
                        Some(Value::Bool(true)) => !descendants.is_empty(),
                        _ => descendants.is_empty(),
                    }
                })
            }
            "habit" => self.split(items, from, text("Habits".to_string()), |facts| {
                let (Some(outline), Some((index, _))) = (facts.outline, facts.entry) else { return false };
                let inherit = outline.inheritance.inherits_property("STYLE");
                outline.entry_property(Some(index), "STYLE", inherit).as_deref() == Some("habit")
            }),
            "file-path" => {
                let patterns = strings(&args);
                let regexes: Vec<Regex> = patterns.iter().filter_map(|pattern| emacs_regex(pattern)).collect();
                let name = format!("File path: {}", patterns.join(" OR "));
                self.split(items, from, text(name), move |facts| {
                    facts.path.is_some_and(|path| regexes.iter().any(|regex| regex.is_match(path)))
                })
            }
            "log" => {
                let arg = first(&args);
                let (name, wanted): (String, Option<&str>) = match arg.and_then(Value::as_str) {
                    Some("close" | "closed") => ("Log: Closed".to_string(), Some("closed")),
                    Some("clock" | "clocked") => ("Log: Clocked".to_string(), Some("clock")),
                    Some("changed" | "state") => ("Log: State changed".to_string(), Some("state")),
                    _ if is_t(arg) => ("Logged".to_string(), None),
                    _ => ("Not logged".to_string(), None),
                };
                let any = is_t(arg);
                self.split(items, from, text(name), move |facts| match wanted {
                    Some(kind) => facts.item.log == Some(kind),
                    None if any => facts.item.log.is_some(),
                    None => facts.item.log.is_none(),
                })
            }
            "heading-regexp" | "regexp" => {
                let patterns = strings(&args);
                let regexes: Vec<Regex> = patterns.iter().filter_map(|pattern| emacs_regex(pattern)).collect();
                let quoted = patterns.iter().map(|pattern| format!("\"{pattern}\"")).collect::<Vec<_>>().join(" OR ");
                let name = if key == "regexp" {
                    format!("Items matching regexps: {quoted}")
                } else {
                    format!("Headings matching regexps: {quoted}")
                };
                let whole = key == "regexp";
                self.split(items, from, text(name), move |facts| {
                    let (Some(outline), Some((index, entry))) = (facts.outline, facts.entry) else { return false };
                    let searched = if whole { entry_text(outline, index) } else { heading(outline, entry) };
                    regexes.iter().any(|regex| regex.is_match(&searched))
                })
            }
            "pred" | "auto-map" => return Err(unsupported(key)),
            "property" => {
                let name = strings(&args).first().cloned().unwrap_or_default();
                let wanted = match args.get(1) {
                    None | Some(Value::Null) => None,
                    Some(Value::String(value)) => Some(value.clone()),
                    Some(_) => return Err(unsupported("property")),
                };
                let title =
                    format!("Property: {name}{}", wanted.as_ref().map_or(String::new(), |value| format!(": {value}")));
                let inherit = self.settings.properties_inherit;
                self.split(items, from, text(title), move |facts| {
                    let found = entry_get(facts, &name, inherit);
                    match (&found, &wanted) {
                        (None, _) => false,
                        (Some(_), None) => true,
                        (Some(found), Some(wanted)) => found == wanted,
                    }
                })
            }
            "tag" => {
                let tags = strings(&args);
                let name = format!("Tags: {}", tags.join(" OR "));
                self.split(items, from, text(name), move |facts| {
                    item_tags(facts).iter().any(|tag| tags.iter().any(|wanted| equalp(tag, wanted)))
                })
            }
            "category" => {
                let categories = strings(&args);
                let name = format!("Items categorized as: {}", categories.join(" OR "));
                self.split(items, from, text(name), move |facts| {
                    facts.entry.is_some_and(|(_, entry)| categories.contains(&entry.category))
                })
            }
            "todo" => {
                let arg = first(&args);
                let (name, mode) = match arg {
                    Some(Value::String(_)) => (format!("{} items", strings(&args).join(" and ")), 0),
                    _ if is_t(arg) => ("Any TODO keyword".to_string(), 1),
                    _ if is_nil(arg) => ("Non-todo items".to_string(), 2),
                    _ => return Err("Argument to `:todo' must be a string, list of strings, t, or nil".to_string()),
                };
                let keywords = strings(&args);
                self.split(items, from, text(name), move |facts| {
                    let state = todo_state(facts);
                    match mode {
                        0 => state.is_some_and(|state| keywords.iter().any(|keyword| keyword == state)),
                        1 => state.is_some(),
                        _ => state.is_none(),
                    }
                })
            }
            "priority" => {
                let wanted = strings(&args);
                let name = format!("Priority {} items", wanted.join(" and "));
                self.split(items, from, text(name), move |facts| {
                    priority_cookie(&facts.item.text).is_some_and(|cookie| wanted.contains(&cookie))
                })
            }
            "priority>" | "priority>=" | "priority<" | "priority<=" => {
                let wanted = strings(&args);
                let limit = wanted.first().and_then(|text| text.chars().next()).unwrap_or('\0');
                let name = format!("Priority {} {} items", &key[8..], wanted.join(" or "));
                let operator = key[8..].to_string();
                self.split(items, from, text(name), move |facts| {
                    let Some(cookie) = priority_cookie(&facts.item.text) else { return false };
                    let mine = cookie.chars().next().unwrap_or('\0');
                    // A higher priority is a lower character.
                    match operator.as_str() {
                        ">" => mine < limit,
                        ">=" => mine <= limit,
                        "<" => mine > limit,
                        _ => mine >= limit,
                    }
                })
            }
            "and" => {
                let inner = selectors(value)?;
                let mut matches: Vec<Vec<usize>> = Vec::new();
                let mut names = Vec::new();
                for (key, args) in &inner {
                    if SPECIAL.contains(&key.as_str()) {
                        continue;
                    }
                    let outcome = self.select(items, from, key, args)?;
                    if let Name::Text(name) = &outcome.name {
                        names.push(name.clone());
                    }
                    matches.push(match outcome.found {
                        Matched::Items(found) => found,
                        Matched::Groups(groups) => groups.into_iter().flat_map(|(_, members)| members).collect(),
                    });
                }
                // `seq-intersection` in turn, in the order of the first.
                let mut found = matches.first().cloned().unwrap_or_default();
                for other in matches.iter().skip(1) {
                    found.retain(|index| other.contains(index));
                }
                let rest = from.iter().filter(|index| !found.contains(index)).copied().collect();
                Outcome { name: Name::Text(names.join(" AND ")), rest, found: Matched::Items(found) }
            }
            "not" => {
                let inner = selectors(value)?;
                let outcome = self.dispatch(items, from, &inner)?;
                let found = match outcome.found {
                    Matched::Items(found) => found,
                    Matched::Groups(groups) => groups.into_iter().flat_map(|(_, members)| members).collect(),
                };
                Outcome { name: outcome.name, rest: found, found: Matched::Items(outcome.rest) }
            }
            "take" => {
                let count = args.first().and_then(Value::as_i64).unwrap_or(0);
                let inner = selectors(args.get(1).unwrap_or(&Value::Null))?;
                let outcome = self.dispatch(items, from, &inner)?;
                let found = match outcome.found {
                    Matched::Items(found) => found,
                    Matched::Groups(groups) => groups.into_iter().flat_map(|(_, members)| members).collect(),
                };
                let size = count.unsigned_abs() as usize;
                let taken = if count < 0 {
                    found[found.len().saturating_sub(size)..].to_vec()
                } else {
                    found.into_iter().take(size).collect()
                };
                let placement = if count < 0 { "Last" } else { "First" };
                let inner_name = match outcome.name {
                    Name::Text(name) => name,
                    Name::Auto(selector) => format!(":{selector}"),
                    Name::None => "nil".to_string(),
                };
                let name = format!("{placement} {} {inner_name}", count.abs());
                Outcome { name: Name::Text(name), rest: outcome.rest, found: Matched::Items(taken) }
            }
            "discard" => {
                let inner = selectors(value)?;
                let outcome = self.dispatch(items, from, &inner)?;
                let name = match outcome.name {
                    Name::Text(name) => Name::Text(name),
                    _ => Name::None,
                };
                Outcome { name, rest: outcome.rest, found: Matched::Items(Vec::new()) }
            }
            // Auto groups.
            "auto-planning" => {
                let format = self.settings.date_format.clone();
                self.auto(
                    items,
                    from,
                    key,
                    |facts| {
                        let (_, entry) = facts.entry?;
                        let stamps = [&entry.scheduled, &entry.deadline]
                            .into_iter()
                            .flatten()
                            .filter_map(|planning| stamp_time(&planning.raw));
                        let earliest = stamps.min()?;
                        Some((crate::capture::format_time(&format, earliest), earliest))
                    },
                    |a, b| a.1.cmp(&b.1),
                    str::to_string,
                )
            }
            "auto-tags" => self.auto_by_name(
                items,
                from,
                key,
                |facts| {
                    let mut tags = item_tags(facts);
                    if tags.is_empty() {
                        return None;
                    }
                    tags.sort();
                    Some(format!("Tags: {}", tags.join(", ")))
                },
                str::to_string,
            ),
            "auto-ts" => {
                let format = self.settings.date_format.clone();
                let reverse = args.iter().any(|arg| arg.as_str() == Some("reverse"));
                self.auto(
                    items,
                    from,
                    key,
                    |facts| {
                        let (Some(outline), Some((index, entry))) = (facts.outline, facts.entry) else { return None };
                        let start = org_marker(&outline.text, facts.item).unwrap_or(entry.offset);
                        let end = entry_end(outline, index);
                        let latest = latest_stamp(&outline.text[start.min(end)..end])?;
                        Some((crate::capture::format_time(&format, latest), latest))
                    },
                    move |a, b| if reverse { b.1.cmp(&a.1) } else { a.1.cmp(&b.1) },
                    str::to_string,
                )
            }
            "auto-group" => {
                let property = self.settings.group_property_name.clone();
                let inherit = self.settings.properties_inherit;
                self.auto_by_name(
                    items,
                    from,
                    key,
                    |facts| entry_get(facts, &property, inherit),
                    |key| format!("Group: {key}"),
                )
            }
            "auto-category" => self.auto_by_name(
                items,
                from,
                key,
                |facts| facts.entry.map(|(_, entry)| entry.category.clone()),
                |key| format!("Category: {key}"),
            ),
            "auto-priority" => self.auto_by_name(
                items,
                from,
                key,
                |facts| priority_cookie(&facts.item.text),
                |key| format!("Priority: {key}"),
            ),
            "auto-property" => {
                let property = strings(&args).first().cloned().unwrap_or_default();
                let inherit = self.settings.properties_inherit;
                let shown = property.clone();
                self.auto_by_name(
                    items,
                    from,
                    key,
                    move |facts| entry_get(facts, &property, inherit),
                    move |key| format!("{shown}: {key}"),
                )
            }
            "auto-todo" => self.auto_by_name(
                items,
                from,
                key,
                |facts| todo_state(facts).map(str::to_string),
                |key| format!("To-do: {key}"),
            ),
            "auto-dir-name" => self.auto_by_name(
                items,
                from,
                key,
                |facts| {
                    let path = std::path::Path::new(facts.path?);
                    let directory = path.parent()?.file_name()?.to_string_lossy().into_owned();
                    Some(format!("Directory: {directory}"))
                },
                str::to_string,
            ),
            "auto-outline-path" => self.auto_by_name(
                items,
                from,
                key,
                |facts| {
                    let (Some(outline), Some((index, _))) = (facts.outline, facts.entry) else { return None };
                    let path = outline_path(outline, index);
                    Some(if path.is_empty() { "Top-level headings".to_string() } else { path.join("/") })
                },
                str::to_string,
            ),
            "auto-parent" => self.auto_by_name(
                items,
                from,
                key,
                |facts| {
                    let (Some(outline), Some((_, entry))) = (facts.outline, facts.entry) else { return None };
                    let parent = &outline.entries[entry.parent?];
                    Some(title(outline, parent))
                },
                str::to_string,
            ),
            "ancestor-with-todo" => {
                let (keyword, limit, nearest) = match args.first() {
                    Some(Value::Array(list)) => {
                        let options = options(list.get(1..).unwrap_or_default());
                        (
                            list.first().map(lisp).unwrap_or_default(),
                            options.get("limit").and_then(Value::as_i64),
                            options.get("nearestp").is_some_and(|value| !is_nil(Some(value))),
                        )
                    }
                    Some(other) => (lisp(other), None, false),
                    None => ("nil".to_string(), None, false),
                };
                let prefix = if nearest { "Nearest" } else { "Ancestor" };
                let header_keyword = keyword.clone();
                self.auto_by_name(
                    items,
                    from,
                    key,
                    move |facts| {
                        let (Some(outline), Some((_, entry))) = (facts.outline, facts.entry) else { return None };
                        let mut found = None;
                        let mut left = limit;
                        let mut at = entry.parent;
                        while let Some(index) = at {
                            if let Some(count) = left.as_mut() {
                                *count -= 1;
                                if *count < 0 {
                                    break;
                                }
                            }
                            let above = &outline.entries[index];
                            if above.todo.as_deref() == Some(keyword.as_str()) {
                                found = Some(title(outline, above));
                                if nearest {
                                    break;
                                }
                            }
                            at = above.parent;
                        }
                        found
                    },
                    move |key| format!("{prefix} {header_keyword}: {key}"),
                )
            }
            other => return Err(format!("Invalid org-super-agenda-groups selector: :{other}")),
        })
    }

    /// `:deadline` and `:scheduled`.
    fn planning(&self, items: &[Item], from: &[usize], key: &str, args: &[Value]) -> Result<Outcome, String> {
        let deadline = key == "deadline";
        let arg = first(args);
        let word = arg.and_then(Value::as_str).unwrap_or("");
        let date = args.get(1).and_then(Value::as_str).unwrap_or("");
        let name = match (deadline, word) {
            _ if is_t(arg) => if deadline { "Deadline items" } else { "Scheduled items" }.to_string(),
            _ if is_nil(arg) => if deadline { "Items without deadlines" } else { "Unscheduled items " }.to_string(),
            (true, "past") => "Past due".to_string(),
            (true, "today") => "Due today".to_string(),
            (true, "future") => "Due soon".to_string(),
            (true, "before") => format!("Due before {date}"),
            (true, "on") => format!("Due on {date}"),
            (true, "after") => format!("Due after {date}"),
            (false, "past") => "Past scheduled".to_string(),
            (false, "today") => "Scheduled today".to_string(),
            (false, "future") => "Scheduled soon".to_string(),
            (false, "before") => format!("Scheduled before {date}"),
            (false, "on") => format!("Scheduled on {date}"),
            (false, "after") => format!("Scheduled after {date}"),
            _ => String::new(),
        };
        let target = match word {
            "before" | "on" | "after" => Some(time_string_to_day(date).ok_or_else(|| format!("Not a date: {date}"))?),
            _ => Some(self.today),
        };
        let any = is_t(arg);
        let none = is_nil(arg);
        let word = word.to_string();
        let name = if name.is_empty() { Name::None } else { Name::Text(name) };
        Ok(self.split(items, from, name, move |facts| {
            let Some((_, entry)) = facts.entry else { return false };
            let planning = if deadline { &entry.deadline } else { &entry.scheduled };
            let raw = planning.as_ref().map(|planning| planning.raw.as_str());
            if any {
                return raw.is_some();
            }
            if none {
                return raw.is_none();
            }
            let (Some(day), Some(target)) = (raw.and_then(time_string_to_day), target) else { return false };
            match word.as_str() {
                "past" | "before" => day < target,
                "today" | "on" => day == target,
                "future" | "after" => day > target,
                _ => false,
            }
        }))
    }
}

/// The message for a selector that needs Emacs Lisp.
fn unsupported(selector: &str) -> String {
    format!("org-super-agenda: :{selector} calls Emacs Lisp, which org-lsp does not run")
}

/// `org-super-agenda--transform-groups`: `:order-multi` groups, each
/// with the order given first.
fn transform(groups: &[Value]) -> Result<Vec<Value>, String> {
    let mut out = Vec::new();
    for group in groups {
        let pairs = selectors(group)?;
        match pairs.first() {
            Some((key, Value::Array(list))) if key == "order-multi" => {
                let order = list.first().cloned().unwrap_or(Value::Null);
                for member in list.iter().skip(1) {
                    let mut pairs = selectors(member)?;
                    match pairs.iter_mut().find(|(key, _)| key == "order") {
                        Some(pair) => pair.1 = order.clone(),
                        None => pairs.push(("order".to_string(), order.clone())),
                    }
                    out.push(Value::Array(
                        pairs.into_iter().flat_map(|(key, value)| [Value::String(key), value]).collect(),
                    ));
                }
            }
            _ => out.push(group.clone()),
        }
    }
    Ok(out)
}

/// A plist of options as a map: `["limit", 2]` or `[{"limit": 2}]`.
fn options(list: &[Value]) -> BTreeMap<String, Value> {
    let mut out = BTreeMap::new();
    match list {
        [Value::Object(map)] => {
            for (key, value) in map {
                out.insert(key.trim_start_matches(':').to_string(), value.clone());
            }
        }
        _ => {
            for pair in list.chunks(2) {
                if let Some(key) = pair[0].as_str() {
                    out.insert(key.trim_start_matches(':').to_string(), pair.get(1).cloned().unwrap_or(Value::Null));
                }
            }
        }
    }
    out
}

/// Strings equal as `cl-equalp` has them: without regard to case.
fn equalp(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.to_lowercase()
}

/// The tags of an agenda line (its `tags` property).
fn item_tags(facts: &Facts) -> Vec<String> {
    match (facts.item.kind, facts.entry) {
        (ItemKind::Grid, _) | (_, None) => Vec::new(),
        (_, Some((_, entry))) => entry.tags.iter().map(|tag| tag.name.clone()).collect(),
    }
}

/// The TODO state of an agenda line (its `todo-state` property, which
/// lines of log mode and the time grid have none of).
fn todo_state<'a>(facts: &Facts<'a>) -> Option<&'a str> {
    match facts.item.kind {
        ItemKind::Grid | ItemKind::Log | ItemKind::Search => None,
        // `org-agenda-get-deadlines` and `-scheduled` read the element,
        // the others `org-get-todo-state`: tabs around the keyword differ.
        ItemKind::Deadline | ItemKind::Scheduled => facts.entry.and_then(|(_, entry)| entry.element_todo.as_deref()),
        _ => facts.entry.and_then(|(_, entry)| entry.todo.as_deref()),
    }
}

/// The first priority cookie of a line (`org-priority-regexp`).
fn priority_cookie(text: &str) -> Option<String> {
    static COOKIE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)\[#([A-Z]|[0-9]|[1-5][0-9]|6[0-4])\]").expect("a valid regex"));
    COOKIE.captures(text).map(|captures| captures[1].to_string())
}

/// `org-duration-to-minutes` of an effort, as `org-duration-string-to-minutes`.
fn duration_minutes(text: &str) -> Option<f64> {
    crate::clock::duration_to_minutes(text.trim())
}

/// `org-entry-get` at the entry of a line, with the special properties
/// Org keeps outside the drawer.
fn entry_get(facts: &Facts, name: &str, inherit: bool) -> Option<String> {
    let (outline, (index, entry)) = (facts.outline?, facts.entry?);
    let planning = |planning: &Option<crate::outline::Planning>| planning.as_ref().map(|planning| planning.raw.clone());
    match name.to_uppercase().as_str() {
        "TODO" => entry.todo.clone(),
        "ITEM" => Some(title(outline, entry)),
        "CATEGORY" => Some(entry.category.clone()),
        "DEADLINE" => planning(&entry.deadline),
        "SCHEDULED" => planning(&entry.scheduled),
        "CLOSED" => planning(&entry.closed),
        "PRIORITY" => {
            let line = &outline.text[entry.offset..entry.line_end];
            Some(priority_cookie(line).unwrap_or_else(|| {
                let priorities = outline.priorities.unwrap_or_default();
                crate::properties::priority_string(priorities.default)
            }))
        }
        _ => outline.entry_property(Some(index), name, inherit),
    }
}

/// Where the `org-marker` of an item points. A deadline's or scheduled
/// item's is at the first `DEADLINE: <` or `SCHEDULED: <` of its planning
/// line, though its date is the element's, the last one.
fn org_marker(text: &str, item: &Item) -> Option<usize> {
    static DEADLINE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\bDEADLINE: *<[^>]+>").expect("a valid regex"));
    static SCHEDULED: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\bSCHEDULED: *<[^>]+>").expect("a valid regex"));
    let marker = item.marker?;
    let regex = match item.kind {
        ItemKind::Deadline => &DEADLINE,
        ItemKind::Scheduled => &SCHEDULED,
        _ => return Some(marker),
    };
    let start = text[..marker].rfind('\n').map_or(0, |at| at + 1);
    let end = text[marker..].find('\n').map_or(text.len(), |at| marker + at);
    let found =
        regex.find(&text[start..end]).and_then(|found| found.as_str().find('<').map(|at| start + found.start() + at));
    Some(found.unwrap_or(marker))
}

/// The title of an entry (`ITEM`): its headline without stars, keyword,
/// priority and tags, its tabs turned into spaces (`org-remove-tabs`).
fn title(outline: &Outline, entry: &Entry) -> String {
    let line = &outline.text[entry.offset..entry.line_end];
    complex_heading_title(line, entry.level, &outline.todo)
        .map_or(String::new(), |(start, end)| remove_tabs(&line[start..end]))
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

/// `org-get-heading` without tags and keyword: the priority and the title.
fn heading(outline: &Outline, entry: &Entry) -> String {
    let line = &outline.text[entry.offset..entry.line_end];
    let parts = complex_heading_parts(line, entry.level, &outline.todo);
    let (priority, title) = (parts.priority, parts.title);
    let title = title.map_or("", |(start, end)| &line[start..end]);
    match priority {
        Some((start, end)) => format!("{} {title}", &line[start..end]),
        None => title.to_string(),
    }
}

/// Where the entry `index` ends: at the next headline of any level.
fn entry_end(outline: &Outline, index: usize) -> usize {
    outline.entries.get(index + 1).map_or(outline.text.len(), |next| next.offset)
}

/// The text of an entry, from its headline to the next one
/// (`org-entry-beginning-position` to `org-entry-end-position`).
fn entry_text(outline: &Outline, index: usize) -> String {
    outline.text[outline.entries[index].offset..entry_end(outline, index)].to_string()
}

/// The entries below `index`.
fn descendants(outline: &Outline, index: usize) -> Vec<usize> {
    let level = outline.entries[index].level;
    outline.entries[index + 1..]
        .iter()
        .take_while(|entry| entry.level > level)
        .enumerate()
        .map(|(offset, _)| index + 1 + offset)
        .collect()
}

/// `org-get-outline-path`: the titles of the entries above `index`.
fn outline_path(outline: &Outline, index: usize) -> Vec<String> {
    let mut path = Vec::new();
    let mut at = outline.entries[index].parent;
    while let Some(parent) = at {
        let entry = &outline.entries[parent];
        path.push(crate::refile::headline_path_part(&outline.text, entry.offset, &outline.todo));
        at = outline.entries[parent].parent;
    }
    path.reverse();
    path
}

/// The date and time a timestamp starts at.
fn stamp_time(raw: &str) -> Option<DateTime> {
    static TIME: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^\d{4}-\d{2}-\d{2}(?: +[^\]+0-9>\r\n -]+)?(?: +(\d{1,2}):(\d{2}))?").expect("a valid regex")
    });
    let inner = raw.trim_start_matches(['<', '[']);
    let day = time_string_to_day(raw)?;
    let minute = TIME
        .captures(inner)
        .and_then(|captures| {
            Some(captures.get(1)?.as_str().parse::<i64>().ok()? * 60 + captures[2].parse::<i64>().ok()?)
        })
        .unwrap_or(0);
    Some(DateTime { day, minute })
}

/// The latest timestamp in `text` (`org-element--timestamp-regexp`, read
/// as `ts-parse-org` reads it), up to the first it cannot read.
fn latest_stamp(text: &str) -> Option<DateTime> {
    static STAMP: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"[\[<](\d{4}-\d{2}-\d{2}(?: [^\n]*?)?)[\]>]|<%%\([^>\n]+\)[^\n>]*>").expect("a valid regex")
    });
    let mut latest: Option<DateTime> = None;
    for captures in STAMP.captures_iter(text) {
        let Some(inner) = captures.get(1) else { break };
        let Some(time) = stamp_time(&format!("<{}>", inner.as_str())) else { break };
        latest = Some(latest.map_or(time, |known| known.max(time)));
    }
    latest
}

/// `org-super-agenda--hide-or-show-groups`, after the agenda's filters:
/// the headers of the groups none of whose lines are left go, with the
/// lines of the time grid among them.
pub fn hide_empty_groups(lines: &mut Vec<super::Line>) {
    use super::LineKind;
    let is_header = |line: &super::Line| matches!(line.kind, LineKind::Group { .. });
    let names = |line: &super::Line| matches!(line.kind, LineKind::Group { faced } if faced < line.text.len());
    let mut hidden = vec![false; lines.len()];
    let mut at = 0;
    while at < lines.len() {
        if !is_header(&lines[at]) {
            at += 1;
            continue;
        }
        let start = at;
        while at < lines.len() && is_header(&lines[at]) {
            at += 1;
        }
        // Only separators, such as the final one: no group of its own.
        if !lines[start..at].iter().any(names) {
            continue;
        }
        let mut end = at;
        while end < lines.len() && matches!(lines[end].kind, LineKind::Item | LineKind::EntryText) {
            end += 1;
        }
        let shown =
            lines[at..end].iter().any(|line| line.item.as_ref().is_some_and(|item| item.kind != ItemKind::Grid));
        if !shown {
            hidden[start..end].fill(true);
        }
        at = end;
    }
    let mut index = 0;
    lines.retain(|_| {
        index += 1;
        !hidden[index - 1]
    });
}

#[cfg(test)]
mod tests {
    use org_syntax::TodoKeywords;
    use serde_json::json;

    use super::super::{todo_view, Now};
    use super::*;

    const TEXT: &str = "* TODO [#A] Pay bills :money:\n* TODO Paint shed\n* NEXT Call plumber :house:\n";

    /// The global TODO list of `TEXT`, in the groups of `groups`.
    fn grouped(groups: Value) -> Vec<String> {
        let todo = TodoKeywords::from_document("#+TODO: TODO NEXT | DONE\n").unwrap_or_default();
        let outline = Outline::new(&org_syntax::parse(TEXT, &todo), TEXT, "notes");
        let settings = Settings { groups: groups.as_array().cloned().unwrap_or_default(), ..Settings::default() };
        let config = Config { super_agenda: Some(settings), ..Config::default() };
        let view = todo_view(&[&outline], None, Now { today: 20_000, minute: 600 }, &config);
        view.lines.iter().map(|line| line.text.trim_end().to_string()).collect()
    }

    #[test]
    fn groups_by_selector() {
        let lines = grouped(json!([{ "name": "Important", "priority": "A" }, { "todo": "NEXT" }]));
        let headers: Vec<&str> =
            lines.iter().filter(|line| line.starts_with(' ') && !line.starts_with("  ")).map(String::as_str).collect();
        assert_eq!(headers, [" Important", " NEXT items", " Other items"]);
    }

    #[test]
    fn reads_a_plist_as_an_object() {
        let object = grouped(json!([{ "name": "Mine", "tag": ["house", "money"] }]));
        let plist = grouped(json!([["name", "Mine", "tag", ["house", "money"]]]));
        assert_eq!(object, plist);
    }

    #[test]
    fn refuses_what_calls_lisp() {
        for (group, selector) in [
            (json!({ "pred": "my-test" }), "pred"),
            (json!({ "auto-map": "my-key" }), "auto-map"),
            (json!({ "name": "X", "transformer": "upcase", "todo": true }), "transformer"),
        ] {
            let lines = grouped(json!([group]));
            assert_eq!(lines, [format!("org-super-agenda: :{selector} calls Emacs Lisp, which org-lsp does not run")]);
        }
    }

    #[test]
    fn says_what_emacs_says_for_bad_groups() {
        assert_eq!(grouped(json!([{ "frobnicate": true }])), ["Invalid org-super-agenda-groups selector: :frobnicate"]);
        assert_eq!(grouped(json!([{ "date": "tomorrow" }])), ["Argument to `:date' must be `t', `nil', or `today'"]);
    }

    #[test]
    fn takes_an_empty_order_multi() {
        let lines = grouped(json!([{ "order-multi": [] }]));
        assert_eq!(lines.iter().filter(|line| line.starts_with("  notes:")).count(), 3);
    }

    #[test]
    fn takes_an_empty_ancestor_list() {
        let _ = grouped(json!([{ "ancestor-with-todo": [[]] }]));
    }
}
