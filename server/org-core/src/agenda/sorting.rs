//! How the lines of a day, a TODO list, a tags match or a search are
//! ordered, and how many of them show: `org-agenda-sorting-strategy`
//! (`org-entries-lessp` and its `org-cmp-…` functions) and the limits of
//! `org-agenda-finalize-entries` (`org-agenda-max-entries`, `-todos`,
//! `-tags`, `-effort`).
//!
//! Emacs sorts with `sort`, whose algorithm decides the order where the
//! strategies do not: `category-keep` before another strategy says
//! neither of two entries of different categories comes first, whatever
//! the others say. [`emacs_sort`] is that algorithm, so that the order is
//! Emacs's then too.

use super::{effort, Item, ItemKind};
use crate::clock::duration_to_minutes;
use crate::outline::Outline;
use crate::tags::Sort;

/// The kind of view a strategy or a limit is for, as Org names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewType {
    Agenda,
    Todo,
    Tags,
    Search,
}

impl ViewType {
    pub fn name(self) -> &'static str {
        match self {
            ViewType::Agenda => "agenda",
            ViewType::Todo => "todo",
            ViewType::Tags => "tags",
            ViewType::Search => "search",
        }
    }
}

/// One of the symbols of `org-agenda-sorting-strategy`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Strategy {
    TimeUp,
    TimeDown,
    TimestampUp,
    TimestampDown,
    ScheduledUp,
    ScheduledDown,
    DeadlineUp,
    DeadlineDown,
    TsiaUp,
    TsiaDown,
    TsUp,
    TsDown,
    StatsUp,
    StatsDown,
    PriorityUp,
    PriorityDown,
    UrgencyUp,
    UrgencyDown,
    EffortUp,
    EffortDown,
    CategoryUp,
    CategoryDown,
    CategoryKeep,
    TagUp,
    TagDown,
    TodoStateUp,
    TodoStateDown,
    HabitUp,
    HabitDown,
    AlphaUp,
    AlphaDown,
}

impl Strategy {
    /// The strategy a symbol names. `user-defined-up` and `-down` call
    /// `org-agenda-cmp-user-defined`, Lisp the server does not run: they
    /// get the error Emacs gives when it is not a function.
    pub fn parse(name: &str) -> Result<Strategy, String> {
        use Strategy::*;
        Ok(match name {
            "time-up" => TimeUp,
            "time-down" => TimeDown,
            "timestamp-up" => TimestampUp,
            "timestamp-down" => TimestampDown,
            "scheduled-up" => ScheduledUp,
            "scheduled-down" => ScheduledDown,
            "deadline-up" => DeadlineUp,
            "deadline-down" => DeadlineDown,
            "tsia-up" => TsiaUp,
            "tsia-down" => TsiaDown,
            "ts-up" => TsUp,
            "ts-down" => TsDown,
            "stats-up" => StatsUp,
            "stats-down" => StatsDown,
            "priority-up" => PriorityUp,
            "priority-down" => PriorityDown,
            "urgency-up" => UrgencyUp,
            "urgency-down" => UrgencyDown,
            "effort-up" => EffortUp,
            "effort-down" => EffortDown,
            "category-up" => CategoryUp,
            "category-down" => CategoryDown,
            "category-keep" => CategoryKeep,
            "tag-up" => TagUp,
            "tag-down" => TagDown,
            "todo-state-up" => TodoStateUp,
            "todo-state-down" => TodoStateDown,
            "habit-up" => HabitUp,
            "habit-down" => HabitDown,
            "alpha-up" => AlphaUp,
            "alpha-down" => AlphaDown,
            "user-defined-up" | "user-defined-down" => {
                return Err(format!(
                    "Please set `org-agenda-cmp-user-defined' to a function or remove `{name}' from `org-agenda-sorting-strategy'"
                ))
            }
            _ => return Err(format!("Invalid value {name} in `org-agenda-sorting-strategy'")),
        })
    }
}

/// `org-agenda-sorting-strategy`: a list of strategies for each kind of
/// view, or Org's older form, one list for all of them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Strategies {
    ByView(Vec<(String, Vec<String>)>),
    All(Vec<String>),
}

impl Default for Strategies {
    fn default() -> Self {
        let list = |names: &[&str]| names.iter().map(|name| name.to_string()).collect();
        Strategies::ByView(vec![
            ("agenda".into(), list(&["habit-down", "time-up", "urgency-down", "category-keep"])),
            ("todo".into(), list(&["urgency-down", "category-keep"])),
            ("tags".into(), list(&["urgency-down", "category-keep"])),
            ("search".into(), list(&["category-keep"])),
        ])
    }
}

/// A limit of `org-agenda-max-…`: one for every kind of view, or one for
/// each kind that has its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Limit {
    All(i64),
    ByView(Vec<(String, i64)>),
}

impl Limit {
    /// The limit for a view of `view`, as `org-agenda-finalize-entries`
    /// looks it up.
    fn of(&self, view: ViewType) -> Option<i64> {
        match self {
            Limit::All(limit) => Some(*limit),
            Limit::ByView(limits) => limits.iter().find(|(name, _)| name == view.name()).map(|(_, limit)| *limit),
        }
    }
}

/// The options of sorting and limiting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sorting {
    /// `org-agenda-sorting-strategy`
    pub strategies: Strategies,
    /// `org-agenda-sort-notime-is-late`
    pub notime_is_late: bool,
    /// `org-agenda-sort-noeffort-is-high`
    pub noeffort_is_high: bool,
    /// `org-tags-sort-function`, which `tag-up` and `tag-down` use.
    pub tags_sort: Option<Sort>,
    /// `org-agenda-max-entries`
    pub max_entries: Option<Limit>,
    /// `org-agenda-max-todos`
    pub max_todos: Option<Limit>,
    /// `org-agenda-max-tags`
    pub max_tags: Option<Limit>,
    /// `org-agenda-max-effort`, in minutes.
    pub max_effort: Option<Limit>,
}

impl Default for Sorting {
    fn default() -> Self {
        Sorting {
            strategies: Strategies::default(),
            notime_is_late: true,
            noeffort_is_high: true,
            tags_sort: None,
            max_entries: None,
            max_todos: None,
            max_tags: None,
            max_effort: None,
        }
    }
}

impl Sorting {
    /// `org-set-sorting-strategy`: the strategies of a view of `view`.
    pub fn selected(&self, view: ViewType) -> Result<Vec<Strategy>, String> {
        let names: Vec<String> = match &self.strategies {
            Strategies::All(names) => names.clone(),
            Strategies::ByView(lists) => {
                let of = |name: &str| lists.iter().find(|(key, _)| key == name).map(|(_, names)| names.clone());
                of(view.name()).or_else(|| of("agenda")).unwrap_or_else(|| {
                    ["time-up", "category-keep", "urgency-down"].iter().map(|name| name.to_string()).collect()
                })
            }
        };
        names.iter().map(|name| Strategy::parse(name)).collect()
    }

    /// The sort key `org-agenda-entry-get-agenda-timestamp` reads for a
    /// line of the TODO list or of a tags match, by the strategies of the
    /// view: the property it takes the date from, and the word added to
    /// the line's `type`.
    pub fn timestamp_property(strategies: &[Strategy]) -> (&'static [&'static str], &'static str) {
        use Strategy::*;
        let has = |a: Strategy, b: Strategy| strategies.contains(&a) || strategies.contains(&b);
        if has(ScheduledUp, ScheduledDown) {
            (&["SCHEDULED"], " scheduled")
        } else if has(DeadlineUp, DeadlineDown) {
            (&["DEADLINE"], " deadline")
        } else if has(TsUp, TsDown) {
            (&["TIMESTAMP"], " timestamp")
        } else if has(TsiaUp, TsiaDown) {
            (&["TIMESTAMP_IA"], " timestamp_ia")
        } else if has(TimestampUp, TimestampDown) {
            (&["SCHEDULED", "DEADLINE", "TIMESTAMP", "TIMESTAMP_IA"], "")
        } else {
            (&[], "")
        }
    }
}

/// What the strategies compare of a line: the text properties Emacs
/// reads from it.
struct Keys {
    time: Option<i64>,
    /// `type`, against which the timestamp strategies match their word.
    kind: String,
    ts_date: Option<i64>,
    stats: i64,
    priority: i64,
    urgency: i64,
    effort: Option<f64>,
    category: String,
    tag: Option<String>,
    todo: Option<String>,
    /// The TODO keywords of the line's file (`org-todo-keywords-1`).
    keywords: Vec<String>,
    habit: bool,
    alpha: Option<String>,
}

/// `string<` on strings as Emacs compares them, character by character.
fn string_less(a: &str, b: &str) -> bool {
    a.chars().lt(b.chars())
}

/// The value of `cmp` that says which comes first: -1, +1, or none.
type Cmp = Option<i8>;

fn order<T: PartialOrd>(a: T, b: T) -> Cmp {
    if a < b {
        Some(-1)
    } else if b < a {
        Some(1)
    } else {
        None
    }
}

/// The key of a line, from the entry it stands for.
fn keys(item: &Item, files: &[&Outline]) -> Keys {
    let entry = item.source.map(|source| (files[source.file], &files[source.file].entries[source.entry]));
    let kind = match item.kind {
        ItemKind::Deadline => "deadline".to_string(),
        ItemKind::Scheduled => "scheduled".to_string(),
        ItemKind::Timestamp => "timestamp".to_string(),
        ItemKind::Block => "block".to_string(),
        ItemKind::Sexp => "sexp".to_string(),
        ItemKind::Todo => format!("todo{}", item.ts_type),
        ItemKind::Tags => format!("tagsmatch{}", item.ts_type),
        ItemKind::Search => "search".to_string(),
        ItemKind::Log => item.log.unwrap_or("").to_string(),
        ItemKind::Grid => String::new(),
    };
    // The tags the line carries: the entry's own only when the view does
    // not show inherited ones.
    let tag = entry.and_then(|(outline, entry)| match item.own_tags_only {
        true => entry.tags.iter().rfind(|tag| !tag.inherited).map(|tag| tag.name.clone()),
        false => last_tag(outline, entry),
    });
    let todo = entry.and_then(|(_, entry)| todo_state(item, entry)).map(str::to_string);
    // `org-stats` is on the text of the headline in the Org buffer, and
    // `org-cmp-values` reads it at the end of the line: a line that shows
    // no tags ends with that text, but for a deadline, a search result and
    // the time grid, made of text without it.
    let stats = match entry {
        Some((outline, entry))
            if !matches!(item.kind, ItemKind::Deadline | ItemKind::Search | ItemKind::Grid)
                && super::tag_group(&item.text).is_none() =>
        {
            stats(outline, entry)
        }
        _ => 0,
    };
    Keys {
        time: item.time_of_day,
        kind,
        ts_date: item.ts_date,
        stats,
        priority: item.base_priority,
        urgency: item.priority,
        // `org-cmp-effort` reads it at the start of the headline's text
        // (`txt`), which an empty headline does not have.
        effort: effort_minutes(item, files)
            .filter(|_| item.text.get(item.heading..).is_some_and(|txt| !txt.is_empty())),
        // `org-scan-tags` leaves `org-category` nil for `nil`.
        category: entry
            .filter(|(_, entry)| !(item.kind == ItemKind::Tags && entry.no_category))
            .map(|(_, entry)| entry.category.clone())
            .unwrap_or_default(),
        tag,
        todo,
        keywords: entry.map(|(outline, _)| outline.todo.names().map(str::to_string).collect()).unwrap_or_default(),
        habit: item.habit.is_some(),
        // Every line Org formats has a headline, the time grid's too: its
        // dashes.
        alpha: alpha(item, entry.map(|(outline, _)| outline)),
    }
}

/// `todo-state`: the keyword of the entry, as the syntax tree has it for
/// a planning line and a tags match; none on a line of a search, which
/// Org makes without.
fn todo_state<'a>(item: &Item, entry: &'a crate::outline::Entry) -> Option<&'a str> {
    match item.kind {
        ItemKind::Search => None,
        ItemKind::Deadline | ItemKind::Scheduled | ItemKind::Tags => entry.element_todo.as_deref(),
        _ => entry.todo.as_deref(),
    }
}

/// `effort-minutes`: the effort of the entry, but for the lines Org makes
/// without it, those of a search and of the time grid.
fn effort_minutes(item: &Item, files: &[&Outline]) -> Option<f64> {
    if matches!(item.kind, ItemKind::Search | ItemKind::Grid) {
        return None;
    }
    let source = item.source?;
    effort(&files[source.file].entries[source.entry]).and_then(duration_to_minutes)
}

/// `org-cmp-alpha`'s text: the headline (`org-heading`, to the end of the
/// line, tags too) in lower case, without the TODO keyword and the
/// cookie after it when it starts with a keyword of its file
/// (`org-todo-regexp`); none when the line has no headline text.
fn alpha(item: &Item, outline: Option<&Outline>) -> Option<String> {
    let text = item.text.get(item.heading..).filter(|text| !text.is_empty())?;
    // `\\([ \t]*\\[[a-zA-Z0-9]\\]\\)? *`
    let after_keyword = |rest: &'_ str| -> String {
        let blanks = rest.trim_start_matches([' ', '\t']);
        let bytes = blanks.as_bytes();
        let rest = if bytes.len() >= 3 && bytes[0] == b'[' && bytes[1].is_ascii_alphanumeric() && bytes[2] == b']' {
            &blanks[3..]
        } else {
            rest
        };
        rest.trim_start_matches(' ').to_string()
    };
    let stripped = match outline {
        // `\\`[ \t]*` and a keyword, or the text as it is.
        Some(outline) => {
            let rest = text.trim_start_matches([' ', '\t']);
            match outline.todo.names().find(|keyword| rest.starts_with(keyword)) {
                Some(keyword) => after_keyword(&rest[keyword.len()..]),
                None => text.to_string(),
            }
        }
        // A line of no file, such as the time grid's: no keyword to find.
        None => after_keyword(text.trim_start_matches([' ', '\t'])),
    };
    Some(stripped.to_lowercase())
}

/// `(car (last tags))`: the last of the tags of the entry, its own after
/// those it inherits, each where it first comes (`org-get-tags`).
fn last_tag(outline: &Outline, entry: &crate::outline::Entry) -> Option<String> {
    // A tag of its own that is also inherited stands where it is
    // inherited: the parent's (or the file's) tags tell.
    let from_above = |name: &str| match entry.parent {
        Some(parent) => outline.entries[parent].tags.iter().any(|tag| tag.name == name),
        None => outline.file_tags.iter().any(|tag| tag == name),
    };
    match entry.tags.iter().rfind(|tag| !tag.inherited && !from_above(&tag.name)) {
        Some(tag) => Some(tag.name.clone()),
        // Each of its own is inherited too: the last of those inherited.
        None => entry.tags.last().map(|tag| tag.name.clone()),
    }
}

/// The `org-stats` of an entry (`org-refresh-stats-properties`): the
/// percentage of the last statistics cookie of the headline of the entry
/// or, without one, of the closest of its parents that has one.
fn stats(outline: &Outline, entry: &crate::outline::Entry) -> i64 {
    let mut current = Some(entry);
    while let Some(entry) = current {
        if let Some(value) = cookie(&outline.text[entry.offset..entry.line_end]) {
            return value;
        }
        current = entry.parent.map(|parent| &outline.entries[parent]);
    }
    0
}

/// The value of the last `[n/m]` or `[p%]` of a headline line.
fn cookie(line: &str) -> Option<i64> {
    let mut found = None;
    let bytes = line.as_bytes();
    for (at, _) in line.match_indices('[') {
        let digits = |from: usize| bytes[from..].iter().take_while(|b| b.is_ascii_digit()).count();
        let first = digits(at + 1);
        let after = at + 1 + first;
        let number = |from: usize, len: usize| line[from..from + len].parse::<i64>().unwrap_or(0);
        match bytes.get(after) {
            Some(b'%') if bytes.get(after + 1) == Some(&b']') => found = Some(number(at + 1, first)),
            Some(b'/') => {
                let second = digits(after + 1);
                if bytes.get(after + 1 + second) == Some(&b']') {
                    let numerator = number(at + 1, first);
                    found = Some(if second == 0 {
                        numerator
                    } else {
                        match number(after + 1, second) {
                            0 => 0,
                            denominator => numerator * 100 / denominator,
                        }
                    });
                }
            }
            _ => {}
        }
    }
    found
}

/// `org-entries-lessp`: whether `a` comes before `b`.
fn lessp(a: &Keys, b: &Keys, strategies: &[Strategy], sorting: &Sorting, done: &[String]) -> bool {
    use Strategy::*;
    for strategy in strategies {
        let result = match strategy {
            TimestampUp => cmp_ts(a, b, "", sorting),
            TimestampDown => cmp_ts(b, a, "", sorting),
            ScheduledUp => cmp_ts(a, b, "scheduled", sorting),
            ScheduledDown => cmp_ts(b, a, "scheduled", sorting),
            DeadlineUp => cmp_ts(a, b, "deadline", sorting),
            DeadlineDown => cmp_ts(b, a, "deadline", sorting),
            TsiaUp => cmp_ts(a, b, "timestamp_ia", sorting),
            TsiaDown => cmp_ts(b, a, "timestamp_ia", sorting),
            TsUp => cmp_ts(a, b, "timestamp", sorting),
            TsDown => cmp_ts(b, a, "timestamp", sorting),
            TimeUp => cmp_time(a, b, sorting),
            TimeDown => cmp_time(b, a, sorting),
            StatsUp => order(a.stats, b.stats),
            StatsDown => order(b.stats, a.stats),
            PriorityUp => order(a.priority, b.priority),
            PriorityDown => order(b.priority, a.priority),
            UrgencyUp => order(a.urgency, b.urgency),
            UrgencyDown => order(b.urgency, a.urgency),
            EffortUp => cmp_effort(a, b, sorting),
            EffortDown => cmp_effort(b, a, sorting),
            CategoryUp => cmp_category(a, b),
            CategoryDown => cmp_category(b, a),
            CategoryKeep => cmp_category(a, b).map(|_| 1),
            TagUp => cmp_tag(a, b, sorting),
            TagDown => cmp_tag(b, a, sorting),
            TodoStateUp => cmp_todo_state(a, b, done),
            TodoStateDown => cmp_todo_state(b, a, done),
            HabitUp => cmp_habit(a, b),
            HabitDown => cmp_habit(b, a),
            AlphaUp => cmp_alpha(a, b),
            AlphaDown => cmp_alpha(b, a),
        };
        match result {
            Some(-1) => return true,
            Some(1) => return false,
            _ => {}
        }
    }
    false
}

/// `org-cmp-ts`
fn cmp_ts(a: &Keys, b: &Keys, kind: &str, sorting: &Sorting) -> Cmp {
    let none = if sorting.notime_is_late { i64::MAX } else { -1 };
    let date = |keys: &Keys| keys.ts_date.filter(|_| keys.kind.contains(kind)).unwrap_or(none);
    order(date(a), date(b))
}

/// `org-cmp-time`
fn cmp_time(a: &Keys, b: &Keys, sorting: &Sorting) -> Cmp {
    let none = if sorting.notime_is_late { 9901 } else { -1 };
    order(a.time.unwrap_or(none), b.time.unwrap_or(none))
}

/// `org-cmp-effort`
fn cmp_effort(a: &Keys, b: &Keys, sorting: &Sorting) -> Cmp {
    let none = if sorting.noeffort_is_high { i64::MAX as f64 } else { -1.0 };
    order(a.effort.unwrap_or(none), b.effort.unwrap_or(none))
}

/// `org-cmp-category`
fn cmp_category(a: &Keys, b: &Keys) -> Cmp {
    if string_less(&a.category, &b.category) {
        Some(-1)
    } else if string_less(&b.category, &a.category) {
        Some(1)
    } else {
        None
    }
}

/// `org-cmp-tag`, with `org-tags-sort`.
fn cmp_tag(a: &Keys, b: &Keys, sorting: &Sorting) -> Cmp {
    let before = |x: &str, y: &str| match sorting.tags_sort {
        Some(Sort::Descending) => string_less(y, x),
        _ => string_less(x, y),
    };
    match (&a.tag, &b.tag) {
        (None, None) => None,
        (None, _) => Some(1),
        (_, None) => Some(-1),
        (Some(x), Some(y)) if before(x, y) => Some(-1),
        (Some(x), Some(y)) if before(y, x) => Some(1),
        _ => None,
    }
}

/// `org-cmp-todo-state`: done states first, then by where the keyword is
/// in the file's list, the first first.
fn cmp_todo_state(a: &Keys, b: &Keys, done: &[String]) -> Cmp {
    let keywords = if a.keywords.is_empty() { &b.keywords } else { &a.keywords };
    // `(- (length (member state keywords)))`
    let rank = |state: &Option<String>| -> i64 {
        let state = state.as_deref().unwrap_or("");
        keywords.iter().position(|keyword| keyword == state).map_or(0, |at| -((keywords.len() - at) as i64))
    };
    let is_done = |state: &Option<String>| state.as_ref().is_some_and(|state| done.contains(state));
    match (is_done(&a.todo), is_done(&b.todo)) {
        (true, false) => Some(-1),
        (false, true) => Some(1),
        _ => {
            let (la, lb) = (rank(&a.todo), rank(&b.todo));
            if la < lb {
                Some(-1)
            } else if lb < la {
                Some(1)
            } else {
                None
            }
        }
    }
}

/// `org-cmp-habit-p`
fn cmp_habit(a: &Keys, b: &Keys) -> Cmp {
    match (a.habit, b.habit) {
        (true, false) => Some(-1),
        (false, true) => Some(1),
        _ => None,
    }
}

/// `org-cmp-alpha`
fn cmp_alpha(a: &Keys, b: &Keys) -> Cmp {
    match (&a.alpha, &b.alpha) {
        (None, None) => None,
        (None, _) => Some(1),
        (_, None) => Some(-1),
        (Some(x), Some(y)) if string_less(x, y) => Some(-1),
        (Some(x), Some(y)) if string_less(y, x) => Some(1),
        _ => None,
    }
}

/// `(sort items #'org-entries-lessp)` with `strategies`.
pub fn sort(items: &mut Vec<Item>, files: &[&Outline], strategies: &[Strategy], sorting: &Sorting) {
    if items.len() < 2 {
        return;
    }
    let mut order: Vec<usize> = (0..items.len()).collect();
    sort_members(items, &mut order, files, strategies, sorting);
    let mut slots: Vec<Option<Item>> = std::mem::take(items).into_iter().map(Some).collect();
    *items = order.into_iter().map(|index| slots[index].take().expect("each line once")).collect();
}

/// [`sort`] of the lines `members` of `items` stand for.
pub fn sort_members(
    items: &[Item],
    members: &mut [usize],
    files: &[&Outline],
    strategies: &[Strategy],
    sorting: &Sorting,
) {
    // `org-done-keywords-for-agenda`: those of every file.
    let done: Vec<String> = files
        .iter()
        .flat_map(|outline| outline.todo.names().filter(|name| outline.todo.is_done(name)).map(str::to_string))
        .collect();
    let keys: Vec<Option<Keys>> =
        (0..items.len()).map(|index| members.contains(&index).then(|| keys(&items[index], files))).collect();
    let key = |index: &usize| keys[*index].as_ref().expect("a key for each member");
    emacs_sort(members, |a, b| lessp(key(a), key(b), strategies, sorting, &done));
}

/// The limits of `org-agenda-finalize-entries` on sorted `items`
/// (`org-agenda-limit-entries`): effort, then TODO entries, then tagged
/// entries, then all.
pub fn limit(items: Vec<Item>, files: &[&Outline], view: ViewType, sorting: &Sorting) -> Vec<Item> {
    let entry = |item: &Item| item.source.map(|source| &files[source.file].entries[source.entry]);
    let mut items = items;
    if let Some(max) = sorting.max_effort.as_ref().and_then(|limit| limit.of(view)) {
        let none = if sorting.noeffort_is_high { i64::MAX as f64 } else { -1.0 };
        items = limit_entries(items, max, |item| Some(effort_minutes(item, files).unwrap_or(none)));
    }
    if let Some(max) = sorting.max_todos.as_ref().and_then(|limit| limit.of(view)) {
        items = limit_entries(items, max, |item| entry(item).and_then(|entry| todo_state(item, entry)).map(|_| 1.0));
    }
    if let Some(max) = sorting.max_tags.as_ref().and_then(|limit| limit.of(view)) {
        let tagged = |item: &Item, entry: &crate::outline::Entry| match item.own_tags_only {
            true => entry.tags.iter().any(|tag| !tag.inherited),
            false => !entry.tags.is_empty(),
        };
        items = limit_entries(items, max, |item| entry(item).filter(|entry| tagged(item, entry)).map(|_| 1.0));
    }
    if let Some(max) = sorting.max_entries.as_ref().and_then(|limit| limit.of(view)) {
        items = limit_entries(items, max, |item| item.source.map(|_| 1.0));
    }
    items
}

/// `org-agenda-limit-entries`: the lines while what `value` adds up to
/// stays within `limit`; with a negative limit, the lines without a
/// value as well.
fn limit_entries(items: Vec<Item>, limit: i64, value: impl Fn(&Item) -> Option<f64>) -> Vec<Item> {
    let include = limit < 0;
    let mut sum = 0.0;
    items
        .into_iter()
        .filter(|item| match value(item) {
            Some(value) => {
                sum += value;
                sum <= limit.unsigned_abs() as f64
            }
            None => include,
        })
        .collect()
}

/// `sort` of Emacs 29 (its `tim_sort`, from Python's): runs found in
/// turn, made at least `minrun` long by binary insertion, then merged.
/// `lessp` is the predicate, which need not be a consistent order.
pub fn emacs_sort<T: Copy>(items: &mut [T], lessp: impl Fn(&T, &T) -> bool) {
    let length = items.len();
    if length < 2 {
        return;
    }
    let minrun = compute_minrun(length);
    let mut runs: Vec<(usize, usize)> = Vec::new();
    let mut lo = 0;
    while lo < length {
        let remaining = length - lo;
        let (mut n, descending) = count_run(&items[lo..], &lessp);
        if descending {
            items[lo..lo + n].reverse();
        }
        if n < minrun {
            let force = remaining.min(minrun);
            binary_sort(&mut items[lo..lo + force], n, &lessp);
            n = force;
        }
        runs.push((lo, n));
        merge_collapse(items, &mut runs, &lessp);
        lo += n;
    }
    while runs.len() > 1 {
        let mut n = runs.len() - 2;
        if n > 0 && runs[n - 1].1 < runs[n + 1].1 {
            n -= 1;
        }
        merge_at(items, &mut runs, n, &lessp);
    }
}

/// `merge_compute_minrun`
fn compute_minrun(mut n: usize) -> usize {
    let mut r = 0;
    while n >= 64 {
        r |= n & 1;
        n >>= 1;
    }
    n + r
}

/// `count_run`: how long the run at the start is, and whether it is
/// strictly descending.
fn count_run<T>(items: &[T], lessp: &impl Fn(&T, &T) -> bool) -> (usize, bool) {
    if items.len() == 1 {
        return (1, false);
    }
    let mut n = 2;
    if lessp(&items[1], &items[0]) {
        while n < items.len() && lessp(&items[n], &items[n - 1]) {
            n += 1;
        }
        (n, true)
    } else {
        while n < items.len() && !lessp(&items[n], &items[n - 1]) {
            n += 1;
        }
        (n, false)
    }
}

/// `binarysort`: insert the lines from `start` on into the sorted ones
/// before, each after those it is not less than.
fn binary_sort<T: Copy>(items: &mut [T], start: usize, lessp: &impl Fn(&T, &T) -> bool) {
    for at in start.max(1)..items.len() {
        let pivot = items[at];
        let (mut l, mut r) = (0, at);
        while l < r {
            let p = l + ((r - l) >> 1);
            if lessp(&pivot, &items[p]) {
                r = p;
            } else {
                l = p + 1;
            }
        }
        items.copy_within(l..at, l + 1);
        items[l] = pivot;
    }
}

/// `merge_collapse`: merge runs until their lengths keep the invariants.
fn merge_collapse<T: Copy>(items: &mut [T], runs: &mut Vec<(usize, usize)>, lessp: &impl Fn(&T, &T) -> bool) {
    while runs.len() > 1 {
        let mut n = runs.len() - 2;
        let len = |i: usize, runs: &Vec<(usize, usize)>| runs[i].1;
        if (n > 0 && len(n - 1, runs) <= len(n, runs) + len(n + 1, runs))
            || (n > 1 && len(n - 2, runs) <= len(n - 1, runs) + len(n, runs))
        {
            if len(n - 1, runs) < len(n + 1, runs) {
                n -= 1;
            }
            merge_at(items, runs, n, lessp);
        } else if len(n, runs) <= len(n + 1, runs) {
            merge_at(items, runs, n, lessp);
        } else {
            break;
        }
    }
}

/// `merge_at`: the runs `n` and `n + 1` merged, stably, into one.
fn merge_at<T: Copy>(items: &mut [T], runs: &mut Vec<(usize, usize)>, n: usize, lessp: &impl Fn(&T, &T) -> bool) {
    let (start, first) = runs[n];
    let second = runs[n + 1].1;
    let left: Vec<T> = items[start..start + first].to_vec();
    let right: Vec<T> = items[start + first..start + first + second].to_vec();
    let (mut i, mut j) = (0, 0);
    for slot in &mut items[start..start + first + second] {
        // The right run's line goes first only when it is less.
        if j < right.len() && (i == left.len() || lessp(&right[j], &left[i])) {
            *slot = right[j];
            j += 1;
        } else {
            *slot = left[i];
            i += 1;
        }
    }
    runs[n] = (start, first + second);
    runs.remove(n + 1);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sorts_as_a_stable_sort_with_a_consistent_order() {
        for length in [0, 1, 2, 5, 63, 64, 65, 200, 1000] {
            let mut values: Vec<(u32, usize)> =
                (0..length).map(|i| ((i as u32).wrapping_mul(2_654_435_761) % 17, i)).collect();
            let mut expected = values.clone();
            expected.sort_by_key(|value| value.0);
            emacs_sort(&mut values, |a, b| a.0 < b.0);
            assert_eq!(values, expected, "{length}");
        }
    }

    #[test]
    fn reads_statistics_cookies() {
        assert_eq!(cookie("* TODO Plan [1/4] and [50%]"), Some(50));
        assert_eq!(cookie("* TODO Plan [3/4]"), Some(75));
        assert_eq!(cookie("* TODO Plan [0/0]"), Some(0));
        assert_eq!(cookie("* TODO Plan [x]"), None);
    }
}
