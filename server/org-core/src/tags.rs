//! Setting the tags of a headline as `org-set-tags-command` (`C-c C-q`)
//! does: the tag table of `#+TAGS:` lines or `org-tag-alist`, fast tag
//! selection by key with its exclusive groups (`org-fast-tag-selection`),
//! and the new tags written and aligned (`org-set-tags`). Checked against
//! Emacs in `tests/tags`.

use std::sync::LazyLock;

use regex::Regex;

use crate::text::tag_group;
use crate::todo::{headline_level, tags_padding};
use crate::Refusal;

/// An element of a tag table (`org-tag-alist`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TagEntry {
    /// A tag, and the key that selects it.
    Tag(String, Option<char>),
    /// `{`: mutually exclusive tags follow.
    StartGroup,
    /// `}`
    EndGroup,
    /// `[`: a group tag and the tags it stands for follow.
    StartGroupTag,
    /// `]`
    EndGroupTag,
    /// `:` after the tag that names a group.
    GroupTags,
    /// A new line of the table.
    Newline,
}

/// `org-use-fast-tag-selection`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Fast {
    /// When some tag has a key.
    #[default]
    Auto,
    Always,
    Never,
}

/// `org-tags-sort-function`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sort {
    /// `string<`
    Ascending,
    /// `string>`
    Descending,
}

/// What setting tags needs to know besides the text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// `org-tag-alist`: the table when the file has no `#+TAGS:` line.
    pub alist: Vec<TagEntry>,
    /// `org-tag-persistent-alist`: in every table.
    pub persistent: Vec<TagEntry>,
    /// `org-use-fast-tag-selection`
    pub fast: Fast,
    /// `org-fast-tag-selection-single-key`: a key ends the selection.
    pub single_key: bool,
    /// `org-tags-sort-function`
    pub sort: Option<Sort>,
    /// `org-auto-align-tags`
    pub auto_align: bool,
    /// `org-tags-column`
    pub tags_column: i64,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            alist: Vec::new(),
            persistent: Vec::new(),
            fast: Fast::Auto,
            single_key: false,
            sort: None,
            auto_align: true,
            tags_column: -77,
        }
    }
}

fn is_tag_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '@' | '#' | '%')
}

/// `org-tag-string-to-alist`: a table written as a `#+TAGS:` line,
/// `work(w) { @office(o) @home(h) } [ GTD : Control Persp ]`; lines are
/// `\n`-separated.
pub fn tag_string_to_alist(text: &str) -> Vec<TagEntry> {
    static TOKEN: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\A([\w@#%]+|\{.+?\})(?:\((.)\))?\z").expect("the tag regexp is valid"));
    let mut alist: Vec<TagEntry> = Vec::new();
    let mut group_flag = false;
    for line in text.split('\n').filter(|line| !line.is_empty()) {
        alist.push(TagEntry::Newline);
        let tokens: Vec<&str> = line.split_whitespace().collect();
        for (index, token) in tokens.iter().enumerate() {
            match *token {
                "{" => {
                    alist.push(TagEntry::StartGroup);
                    if tokens.get(index + 2) == Some(&":") {
                        group_flag = true;
                    }
                }
                "}" => {
                    alist.push(TagEntry::EndGroup);
                    group_flag = false;
                }
                "[" => {
                    alist.push(TagEntry::StartGroupTag);
                    if tokens.get(index + 2) == Some(&":") {
                        group_flag = true;
                    }
                }
                "]" => {
                    alist.push(TagEntry::EndGroupTag);
                    group_flag = false;
                }
                ":" => alist.push(TagEntry::GroupTags),
                token => {
                    let Some(found) = TOKEN.captures(token) else { continue };
                    let tag = found[1].to_string();
                    let key = found.get(2).and_then(|key| key.as_str().chars().next());
                    let known = alist.iter().any(|entry| matches!(entry, TagEntry::Tag(name, _) if *name == tag));
                    if group_flag || !known {
                        alist.push(TagEntry::Tag(tag, key));
                    }
                }
            }
        }
    }
    // The first line starts no new line.
    if !alist.is_empty() {
        alist.remove(0);
    }
    alist
}

/// `org--tag-add-to-alist`: the tags of `first` before those of
/// `second`, without the ones `second` has already outside groups.
pub fn add_to_alist(first: &[TagEntry], second: &[TagEntry]) -> Vec<TagEntry> {
    if second.is_empty() {
        return first.to_vec();
    }
    if first.is_empty() {
        return second.to_vec();
    }
    let mut out = Vec::new();
    let mut group_flag = false;
    for entry in first {
        match entry {
            TagEntry::StartGroup | TagEntry::StartGroupTag => {
                group_flag = true;
                out.push(entry.clone());
            }
            TagEntry::EndGroup | TagEntry::EndGroupTag => {
                group_flag = false;
                out.push(entry.clone());
            }
            TagEntry::GroupTags | TagEntry::Newline => out.push(entry.clone()),
            TagEntry::Tag(tag, _) => {
                let known = second.iter().any(|other| matches!(other, TagEntry::Tag(name, _) if name == tag));
                if group_flag || !known {
                    out.push(entry.clone());
                }
            }
        }
    }
    out.extend(second.iter().cloned());
    out
}

/// `org-tag-alist-to-groups`: the group tags of a table (`[ GTD :
/// Control Persp ]`, `{ Context : @home @work }`), each with the tags it
/// stands for.
pub fn alist_to_groups(alist: &[TagEntry]) -> Vec<(String, Vec<String>)> {
    let mut groups = Vec::new();
    // In a group; past its `:` with `append`.
    let (mut in_group, mut append) = (false, false);
    let mut current: Vec<String> = Vec::new();
    for entry in alist {
        match entry {
            TagEntry::StartGroup | TagEntry::StartGroupTag => in_group = true,
            TagEntry::EndGroup | TagEntry::EndGroupTag => {
                if append && !current.is_empty() {
                    let tag = current.remove(0);
                    groups.push((tag, std::mem::take(&mut current)));
                }
                (in_group, append) = (false, false);
                current.clear();
            }
            TagEntry::GroupTags => append = true,
            TagEntry::Tag(tag, _) if in_group => {
                if append {
                    current.push(tag.clone());
                } else {
                    current = vec![tag.clone()];
                }
            }
            _ => {}
        }
    }
    groups
}

/// `org-current-tag-alist`: the file's `#+TAGS:` lines, or else
/// `org-tag-alist`, with `org-tag-persistent-alist` before them.
pub fn current_alist(text: &str, settings: &Settings) -> Vec<TagEntry> {
    static TAGS: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?mi)^[ \t]*#\+TAGS:[ \t]*(.*?)[ \t]*$").expect("the keyword regexp is valid"));
    let lines: Vec<&str> =
        TAGS.captures_iter(text).map(|found| found.get(1).map_or("", |value| value.as_str())).collect();
    let local = if lines.is_empty() { settings.alist.clone() } else { tag_string_to_alist(&lines.join("\n")) };
    add_to_alist(&settings.persistent, &local)
}

/// `org-get-buffer-tags`: the tags of the headlines and of `#+FILETAGS:`,
/// last seen first, as `hash-table-keys` gives them back.
pub fn buffer_tags(text: &str) -> Vec<String> {
    static FILETAGS: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?mi)^[ \t]*#\+FILETAGS:[ \t]*(.*?)[ \t]*$").expect("the keyword regexp is valid")
    });
    let mut tags: Vec<String> = Vec::new();
    for line in text.lines() {
        if headline_level(line).is_none() {
            continue;
        }
        for tag in local_tags(line) {
            if !tags.contains(&tag) {
                tags.push(tag);
            }
        }
    }
    for found in FILETAGS.captures_iter(text) {
        for tag in found[1].split(':').map(str::trim).filter(|tag| !tag.is_empty()) {
            if !tags.iter().any(|known| known == tag) {
                tags.push(tag.to_string());
            }
        }
    }
    tags.reverse();
    tags
}

/// The tags at the end of a headline, in their order.
pub fn local_tags(headline: &str) -> Vec<String> {
    match tag_group(headline) {
        Some((_, start)) => headline[start..]
            .trim_end_matches([' ', '\t'])
            .split(':')
            .filter(|tag| !tag.is_empty())
            .map(str::to_string)
            .collect(),
        None => Vec::new(),
    }
}

/// The table `org-set-tags-command` offers: the current one, or else the
/// tags of the file, then `others` (the agenda files' tags, with
/// `org-complete-tags-always-offer-all-agenda-tags`) it does not have.
pub fn table(text: &str, settings: &Settings, others: &[String]) -> Vec<TagEntry> {
    let current = current_alist(text, settings);
    let local = if current.is_empty() {
        buffer_tags(text).into_iter().map(|tag| TagEntry::Tag(tag, None)).collect()
    } else {
        current
    };
    let others: Vec<TagEntry> = others.iter().map(|tag| TagEntry::Tag(tag.clone(), None)).collect();
    let merged = add_to_alist(&others, &local);
    let mut out = local.clone();
    out.extend(merged.into_iter().filter(|entry| !local.contains(entry)));
    out
}

/// Whether the tags are picked by key (`org-use-fast-tag-selection`).
pub fn uses_fast_selection(table: &[TagEntry], settings: &Settings) -> bool {
    match settings.fast {
        Fast::Always => true,
        Fast::Never => false,
        Fast::Auto => table.iter().any(|entry| matches!(entry, TagEntry::Tag(_, Some(_)))),
    }
}

/// The keys `org-fast-tag-selection` assigns to tags without one.
const KEYS: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ{|}~";

/// The tags of fast selection with their keys, in the order shown, and
/// the exclusive groups.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    pub keys: Vec<(String, char)>,
    pub groups: Vec<Vec<String>>,
}

impl Selection {
    /// The keys and groups of `org-fast-tag-selection` for `table`.
    pub fn new(table: &[TagEntry]) -> Selection {
        let explicit =
            |key: char| table.iter().any(|entry| matches!(entry, TagEntry::Tag(_, Some(own)) if *own == key));
        // The tags always shown: those with keys and those in groups.
        let mut left = KEYS.chars().count() as i64;
        let mut ingroup = false;
        for entry in table {
            match entry {
                TagEntry::StartGroup | TagEntry::StartGroupTag => ingroup = true,
                TagEntry::EndGroup | TagEntry::EndGroupTag => ingroup = false,
                TagEntry::Tag(_, Some(_)) => left -= 1,
                TagEntry::Tag(_, None) if ingroup => left -= 1,
                _ => {}
            }
        }
        let mut free: Vec<char> = KEYS.chars().collect();
        let mut keys: Vec<(String, char)> = Vec::new();
        let mut groups: Vec<Vec<String>> = Vec::new();
        let (mut ingroup, mut intaggroup) = (false, false);
        for entry in table {
            match entry {
                TagEntry::StartGroup => {
                    groups.push(Vec::new());
                    ingroup = true;
                }
                TagEntry::EndGroup => ingroup = false,
                TagEntry::StartGroupTag => intaggroup = true,
                TagEntry::EndGroupTag => intaggroup = false,
                TagEntry::GroupTags | TagEntry::Newline => {}
                TagEntry::Tag(tag, own) => {
                    let key = match own {
                        Some(key) => *key,
                        None => {
                            if !(ingroup || intaggroup) {
                                left -= 1;
                            }
                            let first = tag.strip_prefix('@').unwrap_or(tag).chars().next().unwrap_or(' ');
                            let auto = first.to_lowercase().next().unwrap_or(first);
                            let taken = |key: char| keys.iter().any(|(_, bound)| *bound == key) || explicit(key);
                            if taken(auto) {
                                while free.first().is_some_and(|key| taken(*key)) {
                                    free.remove(0);
                                }
                                free.first().copied().unwrap_or(' ')
                            } else {
                                auto
                            }
                        }
                    };
                    if ingroup {
                        if let Some(group) = groups.last_mut() {
                            group.push(tag.clone());
                        }
                    }
                    if ingroup || intaggroup || own.is_some() || left > 0 {
                        keys.push((tag.clone(), key));
                    }
                }
            }
        }
        Selection { keys, groups }
    }

    /// The tag a key selects.
    pub fn tag(&self, key: char) -> Option<&str> {
        self.keys.iter().find(|(_, bound)| *bound == key).map(|(tag, _)| tag.as_str())
    }
}

/// `org--add-or-remove-tag`: `tag` taken out of `current` when there, or
/// else put first, with the others of its exclusive groups taken out.
pub fn add_or_remove(tag: &str, current: &[String], groups: Option<&[Vec<String>]>) -> Vec<String> {
    if current.iter().any(|known| known == tag) {
        return current.iter().filter(|known| *known != tag).cloned().collect();
    }
    let mut out: Vec<String> = current.to_vec();
    for group in groups.unwrap_or_default() {
        if group.iter().any(|member| member == tag) {
            out.retain(|known| !group.contains(known));
        }
    }
    out.insert(0, tag.to_string());
    out
}

/// The tags in the order fast selection shows them, as Emacs's `sort`
/// orders them with `org-fast-tag-selection`'s predicate: `a` before `b`
/// when `b` is shown after `a`. Tags not shown, or shown more than once,
/// make the predicate no order, so how `sort` works decides.
pub fn sort_shown(current: &[String], selection: &Selection) -> Vec<String> {
    let position = |tag: &str| selection.keys.iter().position(|(shown, _)| shown == tag);
    let before = |a: &String, b: &String| -> bool {
        match position(a) {
            Some(at) => selection.keys[at + 1..].iter().any(|(shown, _)| shown == b),
            None => false,
        }
    };
    let mut tags = current.to_vec();
    tim_sort(&mut tags, &before);
    tags
}

/// Emacs's `sort` (`tim_sort` in `sort.c`) for fewer than 64 elements:
/// the run at the start (reversed when strictly descending), then a
/// binary insertion of each element after it.
fn tim_sort(list: &mut [String], before: &dyn Fn(&String, &String) -> bool) {
    let length = list.len();
    if length < 2 {
        return;
    }
    let mut run = 2;
    if before(&list[1], &list[0]) {
        while run < length && before(&list[run], &list[run - 1]) {
            run += 1;
        }
        list[..run].reverse();
    } else {
        while run < length && !before(&list[run], &list[run - 1]) {
            run += 1;
        }
    }
    for start in run..length {
        let pivot = list[start].clone();
        let (mut low, mut high) = (0, start);
        while low < high {
            let middle = low + (high - low) / 2;
            if before(&pivot, &list[middle]) {
                high = middle;
            } else {
                low = middle + 1;
            }
        }
        list[low..=start].rotate_right(1);
    }
}

/// What a key does in fast tag selection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Ask for the next key.
    Again,
    /// The tags as they stand go to the headline.
    Accept,
    /// Ask for a tag by name (`TAB`).
    Type,
    /// Nothing changes (`C-g`, `q` when no tag has it).
    Quit,
}

/// Where fast tag selection stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct State {
    pub current: Vec<String>,
    /// Exclusive groups apply (`!` turns them off and on).
    pub groups: bool,
    /// The next change ends the selection
    /// (`org-fast-tag-selection-single-key`, `C-c`).
    pub exit_after_next: bool,
}

/// A key pressed in fast tag selection (`org-fast-tag-selection`).
pub fn press(state: &mut State, key: char, selection: &Selection) -> Step {
    let groups = state.groups.then_some(selection.groups.as_slice());
    let mut exit_now = false;
    match key {
        '\r' | '\n' => return Step::Accept,
        '!' => state.groups = !state.groups,
        '\u{3}' => state.exit_after_next = !state.exit_after_next,
        '\u{7}' => return Step::Quit,
        'q' if selection.tag('q').is_none() => return Step::Quit,
        ' ' => {
            state.current.clear();
            exit_now = state.exit_after_next;
        }
        '\t' => return Step::Type,
        key => {
            if let Some(tag) = selection.tag(key) {
                state.current = add_or_remove(tag, &state.current, groups);
                exit_now = state.exit_after_next;
            }
        }
    }
    state.current = sort_shown(&state.current, selection);
    if exit_now {
        Step::Accept
    } else {
        Step::Again
    }
}

/// A tag typed after `TAB`: toggled as a key toggles it.
pub fn type_tag(state: &mut State, tag: &str, selection: &Selection) -> Step {
    if tag.chars().any(|c| !c.is_whitespace()) {
        let groups = state.groups.then_some(selection.groups.as_slice());
        state.current = add_or_remove(tag, &state.current, groups);
    }
    state.current = sort_shown(&state.current, selection);
    if state.exit_after_next {
        Step::Accept
    } else {
        Step::Again
    }
}

/// The tags of an answer, as `org-set-tags-command` reads them: anything
/// that cannot be in a tag separates them.
pub fn parse_tags(answer: &str) -> Vec<String> {
    answer.split(|c: char| !is_tag_char(c)).filter(|tag| !tag.is_empty()).map(str::to_string).collect()
}

/// `org-make-tag-string`
pub fn tag_string(tags: &[String]) -> String {
    if tags.is_empty() {
        String::new()
    } else {
        format!(":{}:", tags.join(":"))
    }
}

/// The headline `org-set-tags-command` works on from `at`: the start of
/// its line.
pub fn heading_at(text: &str, at: usize) -> Result<usize, Refusal> {
    let mut start = text[..at.min(text.len())].rfind('\n').map_or(0, |newline| newline + 1);
    loop {
        let end = text[start..].find('\n').map_or(text.len(), |newline| start + newline);
        if headline_level(&text[start..end]).is_some() {
            return Ok(start);
        }
        if start == 0 {
            return Err(Refusal::Failed("Setting file tags is not supported yet".to_string()));
        }
        start = text[..start - 1].rfind('\n').map_or(0, |newline| newline + 1);
    }
}

/// `org-set-tags`: the headline at `heading` with `tags` instead of its
/// own, aligned to `tags_column` (`org-auto-align-tags`).
pub fn set_tags(text: &str, heading: usize, tags: &[String], settings: &Settings) -> String {
    let mut tags: Vec<String> = tags.to_vec();
    match settings.sort {
        Some(Sort::Ascending) => tags.sort(),
        Some(Sort::Descending) => {
            tags.sort();
            tags.reverse();
        }
        None => {}
    }
    let end = text[heading..].find('\n').map_or(text.len(), |newline| heading + newline);
    let mut line = text[heading..end].to_string();
    if tags != local_tags(&line) {
        // The old tags and the blanks before them go.
        let from = match tag_group(&line) {
            Some((_, start)) => start,
            None => line.len(),
        };
        let kept = line[..from].trim_end_matches([' ', '\t']).len();
        line.truncate(kept);
        // A headline of stars alone gets its space back.
        if headline_level(&line).is_none() {
            line.push(' ');
        }
        if !tags.is_empty() {
            line.push(' ');
            line.push_str(&tag_string(&tags));
        }
    }
    if !tags.is_empty() && settings.auto_align {
        if let Some((blanks, start, padding)) = tags_padding(&line, settings.tags_column) {
            line.replace_range(blanks..start, &padding);
        }
    }
    format!("{}{}{}", &text[..heading], line, &text[end..])
}
