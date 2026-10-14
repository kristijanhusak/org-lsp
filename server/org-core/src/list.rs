//! Plain lists as `org-list.el` sees them: the structure of the list at a
//! place (`org-list-struct`), and a new item in it (`org-insert-item`,
//! with the bullets, indentation and checkboxes put right after, as
//! `org-list-write-struct` does). Checked against Emacs in
//! `tests/meta-return`.

use std::sync::LazyLock;

use regex::Regex;

use crate::Refusal;

use crate::todo::headline_level;

/// `org-item-re`, at the start of a line, with Org's defaults (no
/// alphabetical bullets, `.` and `)` both ending a number).
static ITEM: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?:[ \t]*(?:[-+]|[0-9]+[.)])|[ \t]+\*)(?:[ \t]+|$)").unwrap());

/// `org-list-full-item-re`: bullet, counter, checkbox and tag.
static FULL_ITEM: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"^[ \t]*((?:[-+*]|(?:[0-9]+|[A-Za-z])[.)])(?:[ \t]+|$))",
        r"(?:\[@(?:start:)?([0-9]+|[A-Za-z])\][ \t]*)?",
        r"(?:(\[[ X-]\])(?:[ \t]+|$))?",
        r"(?:(.*)[ \t]+::(?:[ \t]+|$))?",
    ))
    .unwrap()
});

static DRAWER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[ \t]*:((?:\w|[-_])+):[ \t]*$").unwrap());

/// `org-list-forbidden-blocks`
const FORBIDDEN_BLOCKS: [&str; 4] = ["example", "verse", "src", "export"];

/// An item of a list structure (`org-list-struct`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Item {
    pub pos: usize,
    pub ind: isize,
    /// With the blanks after it.
    pub bullet: String,
    pub counter: Option<String>,
    pub checkbox: Option<String>,
    pub tag: Option<String>,
    pub end: usize,
}

pub(crate) type Struct = Vec<Item>;

/// How lists are edited: the options of `org-list.el`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// `org-list-two-spaces-after-bullet-regexp`: the bullets with two
    /// spaces after them.
    pub two_spaces: Option<BulletRegexp>,
    /// `org-list-demote-modify-bullet`: the bullet an item gets when it is
    /// indented, for each bullet (`-`, `+`, `*`, `1.`, `1)`).
    pub demote_bullets: Vec<(String, String)>,
    /// `org-list-indent-offset`: how much further than their parent's
    /// text sub-items are indented.
    pub indent_offset: usize,
    /// The `checkbox` rule of `org-list-automatic-rules`: the statistics
    /// cookies are counted again after a box changes.
    pub checkbox_rule: bool,
    /// The `indent` rule of `org-list-automatic-rules`: indenting the
    /// first item of a list with its children moves the whole list.
    pub indent_rule: bool,
    /// `org-checkbox-hierarchical-statistics`: a cookie counts the boxes
    /// of the items right below it, not those further down.
    pub checkbox_hierarchical: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            two_spaces: None,
            demote_bullets: Vec::new(),
            indent_offset: 0,
            checkbox_rule: true,
            indent_rule: true,
            checkbox_hierarchical: true,
        }
    }
}

/// The regexp of `org-list-two-spaces-after-bullet-regexp`, matched as
/// `string-match` does, ignoring case.
#[derive(Debug, Clone)]
pub struct BulletRegexp(Regex);

impl BulletRegexp {
    /// The regexp written as Emacs writes it; `None` when it is not one.
    pub fn new(emacs: &str) -> Option<BulletRegexp> {
        let translated = crate::agenda::matcher::translate(emacs);
        regex::RegexBuilder::new(&translated).case_insensitive(true).build().ok().map(BulletRegexp)
    }

    pub fn is_match(&self, text: &str) -> bool {
        self.0.is_match(text)
    }
}

impl PartialEq for BulletRegexp {
    fn eq(&self, other: &Self) -> bool {
        self.0.as_str() == other.0.as_str()
    }
}

impl Eq for BulletRegexp {}

// ----- lines

/// `at` moved back to the start of the character it is in. Positions
/// that Emacs's list code computes from stale data can fall anywhere.
fn char_start(text: &str, at: usize) -> usize {
    let mut at = at.min(text.len());
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

pub(crate) fn line_start(text: &str, at: usize) -> usize {
    text[..char_start(text, at)].rfind('\n').map_or(0, |index| index + 1)
}

pub(crate) fn line_end(text: &str, at: usize) -> usize {
    let at = char_start(text, at);
    text[at..].find('\n').map_or(text.len(), |index| at + index)
}

/// The start of the line after the one at `at`, or the end of the text.
fn next_line(text: &str, at: usize) -> usize {
    text[at..].find('\n').map_or(text.len(), |index| at + index + 1)
}

/// The start of the line before the one at `at` (`forward-line -1`).
fn previous_line(text: &str, at: usize) -> usize {
    let start = line_start(text, at);
    if start == 0 {
        0
    } else {
        line_start(text, start - 1)
    }
}

fn line(text: &str, start: usize) -> &str {
    &text[start..line_end(text, start)]
}

fn is_blank(line: &str) -> bool {
    line.bytes().all(|b| b == b' ' || b == b'\t')
}

/// `org-current-text-indentation`: the columns of the line's leading
/// blanks, a tab to the next multiple of 8.
pub(crate) fn indentation(line: &str) -> usize {
    let mut column = 0;
    for b in line.bytes() {
        match b {
            b' ' => column += 1,
            b'\t' => column = (column / 8 + 1) * 8,
            _ => break,
        }
    }
    column
}

fn is_heading(line: &str) -> bool {
    headline_level(line).is_some()
}

fn is_item(line: &str) -> bool {
    ITEM.is_match(line)
}

/// Whether the two lines from `start` are blank, each ending with a line
/// break (`org-list-end-re` matching there).
fn two_blank_lines(text: &str, start: usize) -> bool {
    if start >= text.len() {
        return false;
    }
    let first_end = line_end(text, start);
    if first_end >= text.len() || !is_blank(&text[start..first_end]) {
        return false;
    }
    let second = first_end + 1;
    let second_end = line_end(text, second);
    second_end < text.len() && is_blank(&text[second..second_end])
}

// ----- context

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Context {
    Plain,
    Drawer,
    Block,
    Invalid,
}

/// `org-list-context`: the bounds lists are looked for in around the line
/// at `at`, and what they are.
fn context(text: &str, at: usize) -> (usize, usize, Context) {
    let pos = line_start(text, at);
    let mut up = {
        let mut line_at = pos;
        loop {
            if is_heading(line(text, line_at)) {
                break line_at;
            }
            if line_at == 0 {
                break 0;
            }
            line_at = previous_line(text, line_at);
        }
    };
    let mut down = {
        let mut line_at = next_line(text, pos);
        loop {
            if line_at >= text.len() {
                break text.len();
            }
            if is_heading(line(text, line_at)) {
                break line_at;
            }
            line_at = next_line(text, line_at);
        }
    };
    let mut kind = Context::Plain;
    // A drawer around the line.
    let here = line(text, pos);
    if !DRAWER.is_match(here) && !is_end_line(here) {
        let begin = search_back(text, pos, up, |line| DRAWER.is_match(line));
        if let Some(begin) = begin {
            let after = next_line(text, begin);
            // The search for `:END:` starts on the drawer's own line.
            let end = search_forward(text, begin, down, is_end_line).map_or(down, |end| end.saturating_sub(1));
            if end >= pos {
                up = after;
                down = end;
                kind = Context::Drawer;
            }
        }
    }
    // A block around the line.
    static BLOCK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^[ \t]*#\+(begin|end)_").unwrap());
    static BEGIN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^[ \t]*#\+begin_(\S+)").unwrap());
    if !BLOCK.is_match(here) {
        if let Some(begin) = search_back(text, pos, up, |line| BLOCK.is_match(line)) {
            if let Some(found) = BEGIN.captures(line(text, begin)) {
                let kind_name = found[1].to_lowercase();
                let after = next_line(text, begin);
                let end_line = search_forward(text, after, down, |line| BLOCK.is_match(line));
                let end = end_line.map_or(down, |end| end.saturating_sub(1));
                let closes = end_line.is_some_and(|end| {
                    BLOCK.captures(line(text, end)).is_some_and(|found| found[1].eq_ignore_ascii_case("end"))
                });
                if end >= pos && closes {
                    up = after;
                    down = end;
                    kind =
                        if FORBIDDEN_BLOCKS.contains(&kind_name.as_str()) { Context::Invalid } else { Context::Block };
                }
            }
        }
    }
    (up, down, kind)
}

fn is_end_line(line: &str) -> bool {
    line.trim_start_matches([' ', '\t']).get(..5).is_some_and(|end| end.eq_ignore_ascii_case(":END:"))
}

/// The start of the nearest line before `from` (and from `limit` on) that
/// matches.
fn search_back(text: &str, from: usize, limit: usize, matches: impl Fn(&str) -> bool) -> Option<usize> {
    let mut at = line_start(text, from);
    while at > limit {
        at = previous_line(text, at);
        if matches(line(text, at)) {
            return Some(at);
        }
        if at == 0 {
            break;
        }
    }
    None
}

/// The start of the first line from `from` (before `limit`) that matches.
fn search_forward(text: &str, from: usize, limit: usize, matches: impl Fn(&str) -> bool) -> Option<usize> {
    let mut at = from;
    while at < limit && at < text.len() {
        if matches(line(text, at)) {
            return Some(at);
        }
        let next = next_line(text, at);
        if next == at {
            break;
        }
        at = next;
    }
    None
}

/// `org-at-item-p`: whether the line at `at` starts a list item.
pub fn at_item(text: &str, at: usize) -> bool {
    let start = line_start(text, at.min(text.len()));
    in_item(text, start) == Some(start)
}

// ----- org-in-item-p

/// `org-in-item-p`: the start of the item the line at `at` is in.
pub(crate) fn in_item(text: &str, at: usize) -> Option<usize> {
    let mut pos = line_start(text, at);
    let (up, _, kind) = context(text, pos);
    if kind == Context::Invalid {
        return None;
    }
    let here = line(text, pos);
    let mut reference = if is_blank(here) { 10000 } else { indentation(here) };
    if is_item(here) {
        return Some(pos);
    }
    // Inside two blank lines that end a list: from before them.
    let candidates = [previous_line(text, previous_line(text, pos)), previous_line(text, pos), pos];
    let mut searched = candidates[0];
    for start in candidates {
        if start < searched {
            continue;
        }
        if two_blank_lines(text, start) {
            let end = next_line(text, next_line(text, start));
            if start <= pos && pos < end {
                pos = previous_line(text, start);
                break;
            }
            searched = end;
        }
    }
    loop {
        let current = line(text, pos);
        let ind = indentation(current);
        if is_item(current) && ind < reference {
            return Some(pos);
        }
        if pos <= up || two_blank_lines(text, pos) {
            return None;
        }
        if is_block_end(current) {
            if let Some(begin) = search_back(text, pos, up, is_block_begin) {
                pos = begin;
                continue;
            }
        }
        if is_end_line(current) {
            if let Some(drawer) = search_back(text, pos, up, |line| DRAWER.is_match(line)) {
                pos = drawer;
                continue;
            }
        }
        if is_blank(current) {
            pos = previous_line(text, pos);
            continue;
        }
        if ind == 0 {
            return None;
        }
        if ind < reference {
            reference = ind;
        }
        pos = previous_line(text, pos);
    }
}

fn is_block_end(line: &str) -> bool {
    static END: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^[ \t]*#\+end_").unwrap());
    END.is_match(line)
}

fn is_block_begin(line: &str) -> bool {
    static BEGIN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^[ \t]*#\+begin_").unwrap());
    BEGIN.is_match(line)
}

// ----- org-list-struct

fn item_at(text: &str, pos: usize, ind: usize) -> Item {
    let found = FULL_ITEM.captures(line(text, pos));
    let group = |index: usize| found.as_ref().and_then(|found| found.get(index)).map(|m| m.as_str().to_string());
    let bullet = group(1).unwrap_or_default();
    let unordered = bullet.trim_end().len() == 1 && "-+*".contains(bullet.trim_end());
    Item {
        pos,
        ind: ind as isize,
        tag: if unordered { group(4) } else { None },
        counter: group(2),
        checkbox: group(3),
        bullet,
        end: 0,
    }
}

/// `org-list-struct` for the list the item at `item` is in.
pub(crate) fn structure(text: &str, item: usize) -> Struct {
    let start = line_start(text, item);
    let (up, down, _) = context(text, start);
    let mut text_min_ind = 10000;
    let mut beg_cell = (start, indentation(line(text, start)));
    let mut items: Vec<Item> = Vec::new();
    let mut ends: Vec<(usize, usize)> = Vec::new();
    // 1. Up from the item to the start of the list.
    {
        let mut pos = start;
        loop {
            let current = line(text, pos);
            let ind = indentation(current);
            if pos <= up {
                if is_item(current) {
                    beg_cell = (pos, ind);
                    items.insert(0, item_at(text, pos, ind));
                } else {
                    keep_from(&mut items, beg_cell.0);
                }
                break;
            }
            if two_blank_lines(text, pos) {
                keep_from(&mut items, beg_cell.0);
                break;
            }
            if is_item(current) {
                items.insert(0, item_at(text, pos, ind));
                ends.insert(0, (ind, pos));
                if ind < text_min_ind {
                    beg_cell = (pos, ind);
                }
                pos = previous_line(text, pos);
                continue;
            }
            if is_block_end(current) {
                if let Some(begin) = search_back(text, pos, up, is_block_begin) {
                    pos = begin;
                    continue;
                }
            }
            if is_end_line(current) {
                if let Some(drawer) = search_back(text, pos, up, |line| DRAWER.is_match(line)) {
                    pos = drawer;
                    continue;
                }
            }
            if is_blank(current) {
                pos = previous_line(text, pos);
                continue;
            }
            if ind == 0 {
                keep_from(&mut items, beg_cell.0);
                break;
            }
            if ind < text_min_ind {
                text_min_ind = ind;
            }
            ends.insert(0, (ind, pos));
            pos = previous_line(text, pos);
        }
    }
    // 2. Down from the item to the end of the list.
    let mut items_down: Vec<Item> = Vec::new();
    let mut ends_down: Vec<(usize, usize)> = Vec::new();
    {
        let end_before_blank = |pos: usize| -> usize {
            let trimmed = text[..pos].trim_end_matches([' ', '\r', '\t', '\n']).len();
            (line_end(text, trimmed) + 1).min(down)
        };
        let mut pos = start;
        loop {
            if pos >= down {
                ends_down.push((0, end_before_blank(pos)));
                break;
            }
            if two_blank_lines(text, pos) {
                ends_down.push((0, pos));
                break;
            }
            let current = line(text, pos);
            let ind = indentation(current);
            if is_item(current) {
                items_down.push(item_at(text, pos, ind));
                ends_down.push((ind, pos));
                pos = next_line(text, pos);
                if pos == text.len() && !text.ends_with('\n') {
                    pos = text.len();
                }
                continue;
            }
            if is_blank(current) {
                pos = next_line(text, pos);
                if line_end(text, pos) == text.len() && pos == text.len() {
                    pos = text.len();
                }
                continue;
            }
            if ind <= beg_cell.1 {
                ends_down.push((0, end_before_blank(pos)));
                break;
            }
            if items_down.last().is_some_and(|last| ind as isize <= last.ind) {
                ends_down.push((ind, pos));
            }
            if is_block_begin(current) {
                if let Some(end) = search_forward(text, next_line(text, pos), down, is_block_end) {
                    pos = end;
                }
            } else if DRAWER.is_match(current) {
                if let Some(end) = search_forward(text, next_line(text, pos), down, is_end_line) {
                    pos = end;
                }
            }
            pos = next_line(text, pos);
        }
    }
    let mut all = items;
    all.extend(items_down.into_iter().skip(1));
    let mut end_list = ends;
    end_list.extend(ends_down.into_iter().skip(1));
    let end_list: Vec<(isize, usize)> = end_list.into_iter().map(|(ind, end)| (ind as isize, end)).collect();
    assoc_end(&mut all, &end_list);
    all
}

/// `org-element--list-struct`: the structure org-element gives the list
/// starting at `begin`, scanned forward only, up to `limit`. Unlike
/// [`structure`] it takes in the lists after this one that are less
/// indented.
fn element_structure(text: &str, begin: usize, limit: usize) -> Struct {
    static BLOCK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\A[ \t]*#\+BEGIN(:|_\S+)").unwrap());
    let limit = limit.min(text.len());
    let mut items: Vec<Item> = Vec::new();
    let mut done: Vec<Item> = Vec::new();
    let finish = |mut items: Vec<Item>, mut done: Vec<Item>, end: usize| {
        for item in &mut items {
            item.end = end;
        }
        done.extend(items);
        done.sort_by_key(|item| item.pos);
        done
    };
    let mut pos = begin;
    loop {
        if pos >= limit {
            let trimmed = text[..pos.min(text.len())].trim_end_matches([' ', '\r', '\t', '\n']).len();
            return finish(items, done, next_line(text, line_start(text, trimmed)));
        }
        if two_blank_lines(text, pos) {
            return finish(items, done, pos);
        }
        let current = line(text, pos);
        let ind = indentation(current);
        if is_item(current) {
            while items.last().is_some_and(|last| ind as isize <= last.ind) {
                let mut item = items.pop().expect("an item");
                item.end = pos;
                done.push(item);
            }
            items.push(item_at(text, pos, ind));
        } else if !is_blank(current) {
            let trimmed = text[..pos].trim_end_matches([' ', '\r', '\t', '\n']).len();
            let end = next_line(text, line_start(text, trimmed));
            while items.last().is_some_and(|last| ind as isize <= last.ind) {
                let mut item = items.pop().expect("an item");
                item.end = end;
                done.push(item);
                if items.is_empty() {
                    done.sort_by_key(|item| item.pos);
                    return done;
                }
            }
            if let Some(found) = BLOCK.captures(current) {
                let kind = regex::escape(&found[1]);
                let end = Regex::new(&format!(r"(?i)\A[ \t]*#\+END{kind}[ \t]*$")).expect("a regexp");
                if let Some(at) = search_forward(text, next_line(text, pos), limit, |line| end.is_match(line)) {
                    pos = at;
                }
            } else if DRAWER.is_match(current) {
                if let Some(at) = search_forward(text, next_line(text, pos), limit, |line| {
                    line.trim_matches([' ', '\t']).eq_ignore_ascii_case(":END:")
                }) {
                    pos = at;
                }
            }
        }
        let next = next_line(text, pos);
        if next == pos {
            return finish(items, done, text.len());
        }
        pos = next;
    }
}

/// The checkboxes `org-update-checkbox-count` counts in the list
/// starting at `begin` (up to `limit`), as checked and in all: the
/// children of the item at `item` (or its subtree, `recursive`), or the
/// top items of the list (or all of them).
pub(crate) fn count_boxes(
    text: &str,
    begin: usize,
    limit: usize,
    item: Option<usize>,
    recursive: bool,
) -> (usize, usize) {
    let list = element_structure(text, begin, limit);
    if list.is_empty() {
        return (0, 0);
    }
    let parents_of = parents(&list);
    let prevs_of = prevs(&list);
    let index = item.and_then(|item| list.iter().position(|found| found.pos == item));
    let counted: Vec<usize> = match (item, index) {
        (Some(_), None) => Vec::new(),
        (Some(_), Some(index)) if recursive => (0..list.len())
            .filter(|other| list[*other].pos > list[index].pos && list[*other].pos < list[index].end)
            .collect(),
        (Some(_), Some(index)) => (0..list.len()).filter(|other| parents_of[*other] == Some(index)).collect(),
        (None, _) if recursive => (0..list.len()).collect(),
        (None, _) => {
            let mut chain = vec![0];
            while let Some(next) = prevs_of.iter().position(|prev| *prev == chain.last().copied()) {
                chain.push(next);
            }
            chain
        }
    };
    let boxes: Vec<&str> = counted.iter().filter_map(|index| list[*index].checkbox.as_deref()).collect();
    (boxes.iter().filter(|state| **state == "[X]").count(), boxes.len())
}

/// Drop the items before the one at `start`.
fn keep_from(items: &mut Vec<Item>, start: usize) {
    if let Some(index) = items.iter().position(|item| item.pos == start) {
        items.drain(..index);
    } else {
        items.clear();
    }
}

/// `org-list-struct-assoc-end`
fn assoc_end(items: &mut [Item], end_list: &[(isize, usize)]) {
    let mut endings: &[(isize, usize)] = end_list;
    for item in items.iter_mut() {
        while endings.first().is_some_and(|(_, end)| *end <= item.pos) {
            endings = &endings[1..];
        }
        item.end = endings.iter().find(|(ind, _)| *ind <= item.ind).map_or(0, |(_, end)| *end);
    }
}

/// `org-list-get-next-item`, or else `org-list-get-item-end`: where the
/// item at `pos` of `list` is followed by the next in its sub-list, or
/// where it ends.
pub(crate) fn next_item_or_end(list: &Struct, pos: usize) -> Option<usize> {
    let index = list.iter().position(|item| item.pos == pos)?;
    let prevs = prevs(list);
    Some(prevs.iter().position(|prev| *prev == Some(index)).map_or(list[index].end, |next| list[next].pos))
}

/// `org-list-prevs-alist`: each item's previous item in its sub-list.
fn prevs(list: &Struct) -> Vec<Option<usize>> {
    list.iter().map(|item| list.iter().position(|other| other.end == item.pos)).collect()
}

/// `org-list-parents-alist`: each item's parent.
fn parents(list: &Struct) -> Vec<Option<usize>> {
    let mut out = vec![None; list.len()];
    if list.is_empty() {
        return out;
    }
    let mut ind_to_ori: Vec<(isize, Option<usize>)> = vec![(list[0].ind, None)];
    let mut prev = 0;
    for index in 1..list.len() {
        let ind = list[index].ind;
        let prev_ind = ind_to_ori[0].0;
        if prev_ind > ind {
            match ind_to_ori.iter().position(|(known, _)| *known == ind) {
                Some(found) => {
                    ind_to_ori.drain(..found);
                }
                None => match ind_to_ori.iter().position(|(known, _)| *known < ind) {
                    Some(found) => {
                        ind_to_ori.drain(..found);
                    }
                    None => ind_to_ori = vec![(ind, None)],
                },
            }
            out[index] = ind_to_ori[0].1;
        } else if prev_ind < ind {
            ind_to_ori.insert(0, (ind, Some(prev)));
            out[index] = Some(prev);
        } else {
            out[index] = ind_to_ori[0].1;
        }
        prev = index;
    }
    out
}

/// `org-list-bullet-string`: the bullet with one space after it, or two
/// when `org-list-two-spaces-after-bullet-regexp` matches it.
pub(crate) fn bullet_string(bullet: &str, settings: &Settings) -> String {
    let trimmed = bullet.trim_end_matches([' ', '\t']);
    if trimmed.is_empty() {
        return bullet.to_string();
    }
    let two = settings.two_spaces.as_ref().is_some_and(|regexp| regexp.is_match(bullet));
    format!("{trimmed}{}", if two { "  " } else { " " })
}

fn end_before_blank(text: &str, end: usize) -> usize {
    let trimmed = text[..end.min(text.len())].trim_end_matches([' ', '\r', '\t', '\n']).len();
    line_end(text, trimmed)
}

/// `org-list-separating-blank-lines-number`, with `plain-list-item` at
/// `auto`.
fn separating_blank_lines(
    text: &str,
    pos: usize,
    list: &Struct,
    item: usize,
    prevs: &[Option<usize>],
    md: &mut MatchData,
) -> usize {
    let count_blanks = |at: usize| -> usize {
        let start = line_start(text, at);
        let back = text[..start].trim_end_matches([' ', '\r', '\t', '\n']).len();
        let from = next_line(text, back).min(start);
        if back == 0 && text[..start].trim_end_matches([' ', '\r', '\t', '\n']).is_empty() {
            return text[..start].matches('\n').count();
        }
        text[from..start].matches('\n').count()
    };
    let next = prevs.iter().position(|prev| *prev == Some(item));
    if let Some(next) = next {
        return count_blanks(list[next].pos);
    }
    if prevs[item].is_some() {
        return count_blanks(list[item].pos);
    }
    let end_no_blank = end_before_blank(text, list[item].end);
    if pos > end_no_blank {
        let blanks = count_blanks(pos);
        if blanks > 0 {
            return blanks;
        }
    }
    let top = list[0].pos;
    let mut at = top;
    while at <= end_no_blank && at < text.len() {
        let line_text = line(text, at);
        if is_blank(line_text) && line_end(text, at) <= end_no_blank {
            *md = MatchData::buffer(vec![Some((at, line_end(text, at)))]);
            return 1;
        }
        let next = next_line(text, at);
        if next == at {
            break;
        }
        at = next;
    }
    0
}

// ----- org-insert-item

/// What `org-insert-item` makes of the text.
pub(crate) struct Inserted {
    pub text: String,
    pub cursor: usize,
}

/// `org-insert-item` at byte `pos`, with a checkbox when `checkbox`, and
/// after the item at `pos` when `below`. None when `pos` is not in an
/// item.
/// How a new item goes in: `org-M-RET-may-split-line` for items, and
/// the `plain-list-item` of `org-blank-before-new-entry` (none for
/// `auto`).
#[derive(Debug, Clone, Copy)]
pub(crate) struct ItemOptions {
    pub split: bool,
    pub blank: Option<bool>,
}

impl Default for ItemOptions {
    fn default() -> Self {
        ItemOptions { split: true, blank: None }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn insert_item(
    text: &str,
    pos: usize,
    checkbox: bool,
    below: bool,
    keywords: &org_syntax::TodoKeywords,
    tags_column: i64,
    options: ItemOptions,
    settings: &Settings,
) -> Result<Option<Inserted>, Refusal> {
    let Some(itemp) = in_item(text, pos) else { return Ok(None) };
    let list = structure(text, itemp);
    let prevs_of = prevs(&list);
    let first = {
        let Some(mut at) = list.iter().position(|item| item.pos == itemp) else { return Ok(None) };
        while let Some(prev) = prevs_of[at] {
            at = prev;
        }
        at
    };
    let descriptive =
        !list[first].bullet.trim_end().chars().any(|c| c.is_ascii_alphanumeric()) && list[first].tag.is_some();
    let desc = descriptive.then_some(" :: ");
    let mut buffer = Buffer { text: text.to_string(), point: pos, md: MatchData::default(), settings };
    let mut list = list;
    insert_in_struct(&mut buffer, pos, &mut list, checkbox, below, desc, options);
    let parents_of = parents(&list);
    write_struct(&mut buffer, &mut list, &parents_of, None)?;
    let mut text = buffer.text;
    let mut point = buffer.point;
    if checkbox && settings.checkbox_rule {
        // The cookies change, not the lines: the cursor stays on its line.
        let line_number = text[..point].matches('\n').count();
        text = crate::checkbox::update_counts(&text, keywords, point, tags_column, settings.checkbox_hierarchical);
        point = text.match_indices('\n').nth(line_number.wrapping_sub(1)).map_or(0, |(at, _)| at + 1);
    }
    // After the bullet, the counter, the checkbox and the tag.
    let start = line_start(&text, point);
    let current = line(&text, start).to_string();
    let mut cursor = start;
    if let Some(found) = FULL_ITEM.captures(&current) {
        let whole = found.get(0).expect("a match");
        let ordered = found.get(1).is_some_and(|bullet| bullet.as_str().contains(['.', ')']));
        cursor = match found.get(4) {
            Some(tag) if ordered => start + tag.start(),
            _ => start + whole.end(),
        };
        if desc.is_some() {
            cursor -= 1;
        }
    }
    Ok(Some(Inserted { text, cursor }))
}

/// Text and a point, edited as Emacs edits a buffer, with the match data
/// of the last search: `org-list-struct-apply-struct` goes on with it when
/// an item of the structure is no longer at the start of a line, as Emacs
/// does.
struct Buffer<'s> {
    text: String,
    point: usize,
    md: MatchData,
    settings: &'s Settings,
}

/// The match data: the bounds of each group, in the buffer.
#[derive(Debug, Clone, Default)]
struct MatchData {
    groups: Vec<Option<(usize, usize)>>,
}

impl MatchData {
    fn buffer(groups: Vec<Option<(usize, usize)>>) -> MatchData {
        MatchData { groups }
    }

    /// After a `string-match`: no group but the whole match, and that in
    /// a string.
    fn string() -> MatchData {
        MatchData { groups: vec![None] }
    }

    fn from_captures(found: &regex::Captures, offset: usize) -> MatchData {
        MatchData {
            groups: (0..found.len())
                .map(|index| found.get(index).map(|m| (offset + m.start(), offset + m.end())))
                .collect(),
        }
    }

    /// `(looking-at org-list-full-item-re)` at `at`, which needs the start
    /// of a line.
    fn full_item(text: &str, at: usize) -> Option<MatchData> {
        if at > text.len() || line_start(text, at) != at {
            return None;
        }
        FULL_ITEM.captures(line(text, at)).map(|found| MatchData::from_captures(&found, at))
    }

    fn group(&self, index: usize) -> Option<(usize, usize)> {
        self.groups.get(index).copied().flatten()
    }

    /// `match-string` in the buffer; an error when the bounds are not in
    /// it any more.
    fn string_of(&self, text: &str, index: usize) -> Result<Option<String>, Refusal> {
        match self.group(index) {
            None => Ok(None),
            Some((start, end)) => text
                .get(start..end)
                .map(|found| Some(found.to_string()))
                .ok_or_else(|| Refusal::Failed("Args out of range".to_string())),
        }
    }

    /// `update_search_regs` after a replacement of `start..end` that now
    /// ends at `new_end`.
    fn update(&mut self, start: usize, end: usize, new_end: usize) {
        let change = new_end as isize - end as isize;
        let adjust = |at: usize| {
            if at >= end {
                (at as isize + change) as usize
            } else if at > start {
                start
            } else {
                at
            }
        };
        for group in self.groups.iter_mut().flatten() {
            *group = (adjust(group.0), adjust(group.1));
        }
    }
}

const ARGUMENT_ERROR: &str = "Wrong type argument: integer-or-marker-p, nil";

/// `org-list-insert-item`: the new item in the text and in `list`.
/// With `below`, the new item goes after the item at `pos`, whatever
/// part of it `pos` is in, and nothing of it is cut: as with
/// `org-M-RET-may-split-line` nil and the point after the bullet.
fn insert_in_struct(
    buffer: &mut Buffer,
    pos: usize,
    list: &mut Struct,
    checkbox: bool,
    below: bool,
    after_bullet: Option<&str>,
    options: ItemOptions,
) {
    let text = buffer.text.clone();
    // The item that contains `pos`, or the last one.
    let mut found = None;
    for (index, item) in list.iter().enumerate() {
        if item.pos > pos {
            break;
        }
        if item.end < pos {
            continue;
        }
        found = Some(index);
    }
    let item_index = found.unwrap_or(list.len() - 1);
    let item = list[item_index].pos;
    let item_end = list[item_index].end;
    let item_end_no_blank = end_before_blank(&text, item_end);
    let current = line(&text, item).to_string();
    if let Some(found) = MatchData::full_item(&text, item) {
        buffer.md = found;
    }
    let beforep = match FULL_ITEM.captures(&current) {
        Some(found) => {
            let whole = found.get(0).expect("a match").end();
            let limit = match found.get(4) {
                None => whole,
                Some(tag) if found.get(1).is_some_and(|bullet| bullet.as_str().contains(['.', ')'])) => tag.start(),
                Some(tag) => {
                    let after = &current[tag.end()..];
                    tag.end() + after.len() - after.trim_start_matches([' ', '\t']).len()
                }
            };
            !below && pos <= item + limit
        }
        None => !below,
    };
    let split_line = !below && options.split;
    let prevs_of = prevs(list);
    // `org-list-separating-blank-lines-number`, with the
    // `plain-list-item` of `org-blank-before-new-entry`.
    let blank_nb = match options.blank {
        Some(blank) => usize::from(blank),
        None => separating_blank_lines(&text, pos, list, item_index, &prevs_of, &mut buffer.md),
    };
    let ind = list[item_index].ind.max(0) as usize;
    let bullet = bullet_string(&list[item_index].bullet, buffer.settings);
    let box_text = if checkbox { "[ ] " } else { "" };
    let mut text = text;
    let mut pos = pos;
    // The text after the cursor goes to the new item.
    let text_cut = if !beforep && split_line {
        let mut cut_from = pos;
        if item_end < pos {
            let from = item_end.saturating_sub(1);
            let to = line_end(&text, pos);
            text.replace_range(from..to, "");
            cut_from = from;
        }
        let back = text[..cut_from].trim_end_matches([' ', '\r', '\t', '\n']).len();
        let skipped = text[back..].len() - text[back..].trim_start_matches([' ', '\t']).len();
        pos = back + skipped;
        let end_no_blank = end_before_blank(&text, item_end.min(text.len())).max(back);
        let cut = text[back..end_no_blank].to_string();
        text.replace_range(back..end_no_blank, "");
        Some(cut)
    } else {
        None
    };
    let body = format!(
        "{bullet}{box_text}{}{}",
        after_bullet.unwrap_or(""),
        text_cut.as_deref().map_or("", |cut| cut.trim_start_matches([' ', '\t']))
    );
    let item_sep = "\n".repeat(blank_nb + 1);
    let item_size = ind + body.len() + item_sep.len();
    let size_offset = item_size as isize - text_cut.as_ref().map_or(0, |cut| cut.len()) as isize;
    // Insert the item at the start of the item.
    let inserted = format!("{}{body}{item_sep}", " ".repeat(ind));
    text.insert_str(item, &inserted);
    let shift = |at: usize, by: isize| (at as isize + by) as usize;
    for (index, other) in list.iter_mut().enumerate() {
        let p = other.pos;
        if p < item {
            if other.end > item {
                other.end = shift(other.end, size_offset);
            }
        } else if index == item_index && !beforep && split_line {
            other.pos = p + item_size;
            other.end = shift(other.end, size_offset);
        } else if split_line && !beforep && p >= pos && p <= item_end_no_blank {
            let offset = pos as isize
                - item as isize
                - ind as isize
                - bullet.len() as isize
                - after_bullet.map_or(0, str::len) as isize;
            other.pos = shift(p, -offset);
            other.end = shift(other.end, -offset);
        } else {
            other.pos = shift(p, size_offset);
            other.end = shift(other.end, size_offset);
        }
    }
    list.push(Item {
        pos: item,
        ind: ind as isize,
        bullet: bullet.clone(),
        counter: None,
        checkbox: checkbox.then(|| "[ ]".to_string()),
        tag: None,
        end: item + item_size,
    });
    list.sort_by_key(|item| item.pos);
    buffer.text = text;
    if beforep {
        buffer.point = item;
    } else {
        swap_items(buffer, item, item + item_size, list);
        let prevs_of = prevs(list);
        let index = list.iter().position(|other| other.pos == item).expect("the item");
        let next = prevs_of.iter().position(|prev| *prev == Some(index));
        buffer.point = next.map_or(item, |next| list[next].pos);
    }
}

/// `org-list-get-subtree`: the items below the one at `index`.
fn subtree(list: &Struct, index: usize) -> Vec<usize> {
    let end = list[index].end;
    list.iter().enumerate().skip(index + 1).take_while(|(_, item)| item.pos < end).map(|(at, _)| at).collect()
}

/// `org-list-swap-items`
fn swap_items(buffer: &mut Buffer, beg_a: usize, beg_b: usize, list: &mut Struct) {
    let index_a = list.iter().position(|item| item.pos == beg_a).expect("item A");
    let index_b = list.iter().position(|item| item.pos == beg_b).expect("item B");
    let text = &buffer.text;
    let end_a = list[index_a].end;
    let end_b = list[index_b].end;
    let end_a_no_blank = end_before_blank(text, end_a);
    let end_b_no_blank = end_before_blank(text, end_b);
    let size_a = end_a_no_blank - beg_a;
    let size_b = end_b_no_blank - beg_b;
    let body_a = text[beg_a..end_a_no_blank].to_string();
    let body_b = text[beg_b..end_b_no_blank].to_string();
    let between = text[end_a_no_blank..beg_b].to_string();
    let sub_a: Vec<usize> = std::iter::once(index_a).chain(subtree(list, index_a)).collect();
    let sub_b: Vec<usize> = std::iter::once(index_b).chain(subtree(list, index_b)).collect();
    buffer.text.replace_range(beg_a..end_b_no_blank, &format!("{body_b}{between}{body_a}"));
    for (index, item) in list.iter_mut().enumerate() {
        let pos = item.pos;
        if pos < beg_a {
            continue;
        }
        let end_e = item.end;
        if sub_a.contains(&index) {
            item.pos = pos + end_b_no_blank - end_a_no_blank;
            item.end = end_e + end_b_no_blank - end_a_no_blank;
            if end_e == end_a {
                item.end = end_b;
            }
        } else if sub_b.contains(&index) {
            item.pos = pos + beg_a - beg_b;
            item.end = end_e + beg_a - beg_b;
            if end_e == end_b {
                item.end = beg_a + size_b + (end_a - end_a_no_blank);
            }
        } else if pos < beg_b {
            item.pos = (pos as isize + size_b as isize - size_a as isize) as usize;
            item.end = (end_e as isize + size_b as isize - size_a as isize) as usize;
        }
    }
    list.sort_by_key(|item| item.pos);
}

// ----- org-list-write-struct

/// `org-list-write-struct`: bullets, indentation and checkboxes put right
/// in the text.
fn write_struct(
    buffer: &mut Buffer,
    list: &mut Struct,
    parents_of: &[Option<usize>],
    old: Option<&Struct>,
) -> Result<(), Refusal> {
    let old = old.cloned().unwrap_or_else(|| list.clone());
    fix_ind(list, parents_of, Some(2), buffer.settings);
    fix_item_end(list);
    let prevs_of = prevs(list);
    if fix_bul(list, &prevs_of, buffer.settings) {
        buffer.md = MatchData::string();
    }
    fix_ind(list, parents_of, None, buffer.settings);
    fix_box(list, parents_of);
    apply_struct(buffer, list, &old)
}

/// `org-list-struct-fix-ind`: an item indented past its parent's bullet
/// (or `bullet_size`) and `org-list-indent-offset`.
fn fix_ind(list: &mut Struct, parents_of: &[Option<usize>], bullet_size: Option<usize>, settings: &Settings) {
    let top_ind = list[0].ind;
    for index in 1..list.len() {
        list[index].ind = match parents_of[index] {
            Some(parent) => {
                (bullet_size.unwrap_or(list[parent].bullet.len()) + settings.indent_offset) as isize + list[parent].ind
            }
            None => top_ind,
        };
    }
}

/// `org-list-struct-fix-item-end`
fn fix_item_end(list: &mut Struct) {
    let mut end_list: Vec<(isize, usize)> = Vec::new();
    let mut acc_end: Vec<(usize, usize)> = Vec::new();
    for index in 0..list.len() {
        let (pos, ind, end) = (list[index].pos, list[index].ind, list[index].end);
        if !list.iter().any(|item| item.pos == end) {
            // The last item whose end is further than this one's.
            let up = acc_end.iter().find(|(acc, _)| end < *acc).map(|(_, item)| *item);
            let ind = up.and_then(|up| list.iter().find(|item| item.pos == up)).map_or(0, |item| item.ind + 2);
            end_list.insert(0, (ind, end));
        }
        end_list.insert(0, (ind, pos));
        acc_end.insert(0, (end, pos));
    }
    end_list.sort_by_key(|(_, at)| *at);
    assoc_end(list, &end_list);
}

/// `org-list-inc-bullet-maybe`
fn inc_bullet(bullet: &str) -> String {
    static NUMBER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[0-9]+").unwrap());
    static LETTER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[A-Za-z]").unwrap());
    if let Some(found) = NUMBER.find(bullet) {
        let number: u64 = found.as_str().parse().unwrap_or(0);
        return format!("{}{}{}", &bullet[..found.start()], number + 1, &bullet[found.end()..]);
    }
    if let Some(found) = LETTER.find(bullet) {
        let next = char::from(found.as_str().as_bytes()[0] + 1);
        return format!("{}{next}{}", &bullet[..found.start()], &bullet[found.end()..]);
    }
    bullet.to_string()
}

/// `org-list-struct-fix-bul`, without alphabetical bullets. Whether a
/// `string-match` of it matched, which leaves the match data on a string.
fn fix_bul(list: &mut Struct, prevs_of: &[Option<usize>], settings: &Settings) -> bool {
    let alnum = |text: &str| text.chars().any(|c| c.is_ascii_alphanumeric());
    let mut matched = false;
    static NUMBER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[0-9]+").unwrap());
    static NUMBER_OR_LETTER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[0-9]+|[A-Za-z]").unwrap());
    for index in 0..list.len() {
        let prev = prevs_of[index];
        let counter = list[index].counter.clone();
        let bullet = list[index].bullet.clone();
        let numeric = |text: &str| NUMBER.is_match(text);
        matched |= match prev {
            Some(prev) => counter.as_deref().is_some_and(alnum) || alnum(&list[prev].bullet),
            None => counter.as_deref().is_some_and(numeric) || alnum(&bullet),
        };
        let new = match (prev, &counter) {
            (Some(prev), Some(counter)) if numeric(counter) && numeric(&list[prev].bullet) => {
                NUMBER.replace(&list[prev].bullet, counter.as_str()).into_owned()
            }
            (Some(prev), _) => inc_bullet(&list[prev].bullet),
            (None, Some(counter)) if numeric(counter) && numeric(&bullet) => {
                NUMBER.replace(&bullet, counter.as_str()).into_owned()
            }
            (None, _) if NUMBER_OR_LETTER.is_match(&bullet) => NUMBER_OR_LETTER.replace(&bullet, "1").into_owned(),
            (None, _) => bullet,
        };
        list[index].bullet = bullet_string(&new, settings);
    }
    matched
}

/// `org-list-struct-fix-box`, not ordered.
fn fix_box(list: &mut Struct, parents_of: &[Option<usize>]) {
    fix_box_ordered(list, parents_of, false);
}

/// `org-list-struct-fix-box` with `ordered` (an `ORDERED` entry): no box
/// checked after an unchecked one. Returns the item that blocked boxes
/// after it.
fn fix_box_ordered(list: &mut Struct, parents_of: &[Option<usize>], ordered: bool) -> Option<usize> {
    let mut parent_list: Vec<usize> = Vec::new();
    for &parent in parents_of.iter().take(list.len()).flatten() {
        if list[parent].checkbox.is_some() && !parent_list.contains(&parent) {
            parent_list.insert(0, parent);
        }
    }
    parent_list.sort_by(|a, b| list[*b].ind.cmp(&list[*a].ind));
    for parent in parent_list {
        let boxes: Vec<Option<String>> = (0..list.len())
            .filter(|child| parents_of[*child] == Some(parent))
            .map(|child| list[child].checkbox.clone())
            .collect();
        let has = |state: &str| boxes.iter().any(|found| found.as_deref() == Some(state));
        list[parent].checkbox = if (has("[ ]") && has("[X]")) || has("[-]") {
            Some("[-]".to_string())
        } else if has("[X]") {
            Some("[X]".to_string())
        } else if has("[ ]") {
            Some("[ ]".to_string())
        } else {
            list[parent].checkbox.clone()
        };
    }
    if !ordered {
        return None;
    }
    let first_unchecked = list.iter().position(|item| item.checkbox.as_deref() == Some("[ ]"))?;
    if !list[first_unchecked..].iter().any(|item| item.checkbox.as_deref() == Some("[X]")) {
        return None;
    }
    for item in &mut list[first_unchecked..] {
        if item.checkbox.is_some() {
            item.checkbox = Some("[ ]".to_string());
        }
    }
    fix_box_ordered(list, parents_of, false);
    Some(list[first_unchecked].pos)
}

/// `org-list-struct-apply-struct`: the differences between `old` and
/// `list` made in the text.
fn apply_struct(buffer: &mut Buffer, list: &Struct, old: &Struct) -> Result<(), Refusal> {
    let mut origin = buffer.point;
    let find = |list: &Struct, pos: usize| list.iter().find(|item| item.pos == pos).cloned();
    // 1. The shift of each item, and the ends that are not items.
    let mut itm_shift: Vec<(usize, isize, isize)> = Vec::new();
    let mut end_list: Vec<(usize, Option<usize>)> = Vec::new();
    let mut acc_end: Vec<(usize, usize)> = Vec::new();
    for e in old {
        let new = find(list, e.pos).unwrap_or_else(|| e.clone());
        let shift = (new.ind + new.bullet.len() as isize) - (e.ind + e.bullet.len() as isize);
        itm_shift.insert(0, (e.pos, shift, new.ind));
        if !old.iter().any(|item| item.pos == e.end) {
            let up = acc_end.iter().find(|(acc, _)| e.end < *acc).map(|(_, item)| *item);
            end_list.insert(0, (e.end, up));
        }
        acc_end.insert(0, (e.end, e.pos));
    }
    // 2. Slices shifted by the same amount.
    let mut all_ends: Vec<usize> = itm_shift.iter().map(|(pos, _, _)| *pos).collect();
    let mut ends: Vec<usize> = Vec::new();
    for (end, _) in &end_list {
        if !ends.contains(end) {
            ends.push(*end);
        }
    }
    all_ends.extend(ends);
    all_ends.sort();
    acc_end.reverse();
    let mut slices: Vec<(usize, usize, isize, Option<isize>)> = Vec::new();
    for pair in all_ends.windows(2) {
        let (up, down) = (pair[0], pair[1]);
        let itemp = list.iter().any(|item| item.pos == up);
        let delta = if itemp {
            itm_shift.iter().find(|(pos, _, _)| *pos == up).map_or(0, |(_, shift, _)| *shift)
        } else {
            let child = acc_end.iter().find(|(end, _)| *end == up).map(|(_, item)| *item);
            let ind = child.and_then(|child| find(list, child)).map_or(0, |item| item.ind);
            ind - min_indentation(buffer, up, down)
        };
        let ind = itemp.then(|| itm_shift.iter().find(|(pos, _, _)| *pos == up).map(|(_, _, ind)| *ind)).flatten();
        slices.insert(0, (down, up, delta, ind));
    }
    // 3. Shift each slice, from the end, and rewrite the items that changed.
    for (end, beg, delta, ind) in slices {
        if delta != 0 {
            shift_body(buffer, &mut origin, end, beg, delta, ind);
        }
        let new = find(list, beg);
        if new.is_some() && new != find(old, beg) {
            modify_item(buffer, &mut origin, beg, list, old)?;
        }
    }
    buffer.point = origin;
    Ok(())
}

/// The smallest indentation of the lines from `up` to `down`, blank lines,
/// blocks' and drawers' contents left out, as in step 2 of
/// `org-list-struct-apply-struct`.
fn min_indentation(buffer: &mut Buffer, up: usize, down: usize) -> isize {
    static BEGIN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\A#\+BEGIN(:|_\S+)").unwrap());
    static DRAWER_AT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)\A[ \t]*:((?:\w|[-_])+):[ \t]*$").unwrap());
    let text = buffer.text.clone();
    let mut min_ind = isize::MAX;
    let mut at = char_start(&text, up);
    while at < down && at < text.len() {
        let rest_of_line = &text[at..line_end(&text, at)];
        if !is_blank(rest_of_line) {
            min_ind = min_ind.min(indentation(line(&text, line_start(&text, at))) as isize);
            let mut skipped = false;
            if let Some(found) = BEGIN.captures(&text[at..]) {
                let kind = regex::escape(&found[1]);
                let end_re = Regex::new(&format!(r"(?mi)^[ \t]*#\+END{kind}[ \t]*$")).unwrap();
                buffer.md = MatchData::from_captures(&found, at);
                if let Some(end) = search_bounded(&end_re, &text, at, down) {
                    buffer.md = end.0;
                    at = end.1;
                    skipped = true;
                }
            }
            if !skipped && line_start(&text, at) == at {
                if let Some(found) = DRAWER_AT.captures(&text[at..]) {
                    static END: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?mi)^[ \t]*:END:[ \t]*$").unwrap());
                    buffer.md = MatchData::from_captures(&found, at);
                    if let Some(end) = search_bounded(&END, &text, at, down) {
                        buffer.md = end.0;
                        at = end.1;
                    }
                }
            }
        }
        // `forward-line`
        at = next_line(&text, at);
    }
    min_ind
}

/// `re-search-forward` from `from` with the match ending before `bound`:
/// the match data and where it ends.
fn search_bounded(re: &Regex, text: &str, from: usize, bound: usize) -> Option<(MatchData, usize)> {
    let found = re.captures_at(text, from)?;
    let end = found.get(0).expect("a match").end();
    (end <= bound).then(|| (MatchData::from_captures(&found, 0), end))
}

/// Text inserted at `at`; `origin` is a marker, moved when after it or,
/// with `before_markers`, at it.
fn insert(buffer: &mut Buffer, origin: &mut usize, at: usize, what: &str, before_markers: bool) {
    buffer.text.insert_str(at, what);
    if *origin > at || (*origin == at && before_markers) {
        *origin += what.len();
    }
}

fn delete(buffer: &mut Buffer, origin: &mut usize, from: usize, to: usize) {
    buffer.text.replace_range(from..to, "");
    if *origin >= to {
        *origin -= to - from;
    } else if *origin > from {
        *origin = from;
    }
}

/// `replace-match` of group `index`: the point at the end of the new
/// text, the match data adjusted.
fn replace_match(buffer: &mut Buffer, origin: &mut usize, index: usize, new: &str) -> Result<usize, Refusal> {
    let Some((start, end)) = buffer.md.group(index) else {
        return Err(Refusal::Failed("replace-match subexpression does not exist".to_string()));
    };
    if end > buffer.text.len()
        || start > end
        || !buffer.text.is_char_boundary(start)
        || !buffer.text.is_char_boundary(end)
    {
        return Err(Refusal::Failed("Args out of range".to_string()));
    }
    buffer.text.replace_range(start..end, new);
    // `replace_range` moves a marker at the end of the old text to the
    // end of the new, one inside it to its start.
    if *origin >= end {
        *origin = *origin + new.len() - (end - start);
    } else if *origin > start {
        *origin = start;
    }
    buffer.md.update(start, end, start + new.len());
    Ok(start + new.len())
}

/// `indent-line-to` on the line at `at`, with `indent-tabs-mode` off.
fn indent_line_to(buffer: &mut Buffer, origin: &mut usize, at: usize, column: usize) {
    let bol = line_start(&buffer.text, at);
    let current = line(&buffer.text, bol).to_string();
    let blanks = current.len() - current.trim_start_matches([' ', '\t']).len();
    let cur_col = indentation(&current);
    if cur_col < column {
        let mut point = bol + blanks;
        let mut col = cur_col;
        if column - (cur_col / 8) * 8 >= 8 {
            let spaces = current[..blanks].len() - current[..blanks].trim_end_matches(' ').len();
            delete(buffer, origin, point - spaces, point);
            point -= spaces;
            col = indentation(&current[..blanks - spaces]);
        }
        insert(buffer, origin, point, &" ".repeat(column - col), false);
    } else if cur_col > column {
        // `move-to-column` with FORCE: a tab across the column becomes
        // spaces.
        let mut col = 0;
        let mut offset = 0;
        for b in current.bytes() {
            if col >= column {
                break;
            }
            let next = if b == b'\t' { (col / 8 + 1) * 8 } else { col + 1 };
            if next > column {
                let spaces = " ".repeat(next - col);
                delete(buffer, origin, bol + offset, bol + offset + 1);
                insert(buffer, origin, bol + offset, &spaces, false);
                offset += column - col;
                break;
            }
            col = next;
            offset += 1;
        }
        let current = line(&buffer.text, bol).to_string();
        let blanks = current.len() - current.trim_start_matches([' ', '\t']).len();
        if bol + offset < bol + blanks {
            delete(buffer, origin, bol + offset, bol + blanks);
        }
    }
}

/// The body of an item between `beg` and `end` shifted by `delta` columns.
fn shift_body(buffer: &mut Buffer, origin: &mut usize, end: usize, beg: usize, delta: isize, ind: Option<isize>) {
    static ITEM_AT: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?m)\A([ \t]*([-+]|(([0-9]+)[.)]))|[ \t]+\*)([ \t]+|$)").unwrap());
    let end = char_start(&buffer.text, end);
    let trimmed = buffer.text[..end].trim_end_matches([' ', '\r', '\t', '\n']).len();
    let mut at = line_start(&buffer.text, trimmed);
    loop {
        if at < beg {
            break;
        }
        if at == beg {
            if let Some(found) = buffer.text.get(at..).and_then(|rest| ITEM_AT.captures(rest)) {
                buffer.md = MatchData::from_captures(&found, at);
                break;
            }
        }
        let current = line(&buffer.text, at).to_string();
        if !is_blank(&current) {
            let target = (indentation(&current) as isize + delta).max(ind.map_or(-1, |ind| ind + 1)).max(0);
            indent_line_to(buffer, origin, at, target as usize);
        }
        if at == 0 {
            break;
        }
        at = previous_line(&buffer.text, at);
    }
}

/// The first line of the item at `item` rewritten from `list`, as
/// `modify-item` does it: with the match data of the last search when the
/// item is not at the start of a line.
fn modify_item(
    buffer: &mut Buffer,
    origin: &mut usize,
    item: usize,
    list: &Struct,
    old: &Struct,
) -> Result<(), Refusal> {
    let new_item = list.iter().find(|found| found.pos == item).expect("the item");
    let new_ind = new_item.ind;
    let old_ind = indentation(line(&buffer.text, line_start(&buffer.text, item)));
    let new_bullet = bullet_string(&new_item.bullet, buffer.settings);
    let old_bullet = old.iter().find(|found| found.pos == item).map(|found| found.bullet.clone());
    let new_box = new_item.checkbox.clone();
    if let Some(found) = MatchData::full_item(&buffer.text, item) {
        buffer.md = found;
    }
    let mut point = item;
    // a. The bullet.
    if old_bullet.as_deref() != Some(new_bullet.as_str()) {
        let (start, end) = buffer.md.group(1).ok_or_else(|| Refusal::Failed(ARGUMENT_ERROR.to_string()))?;
        let mut keep = String::new();
        if start <= *origin && *origin <= end {
            let rest = buffer.text.get(*origin..).unwrap_or("");
            keep = rest[..rest.len() - rest.trim_start_matches([' ', '\t']).len()].to_string();
        }
        replace_match(buffer, origin, 1, "")?;
        point = buffer.md.group(1).map_or(start, |(_, end)| end);
        insert(buffer, origin, point, &new_bullet, true);
        point += new_bullet.len();
        insert(buffer, origin, point, &keep, false);
    }
    let _ = point;
    if let Some(found) = MatchData::full_item(&buffer.text, item) {
        buffer.md = found;
    }
    point = item;
    // b. The checkbox.
    let old_box = buffer.md.string_of(&buffer.text, 3)?;
    if old_box == new_box {
    } else if let (Some(_), Some(new_box)) = (&old_box, &new_box) {
        point = replace_match(buffer, origin, 3, new_box)?;
    } else if old_box.is_some() {
        static BOX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\A.*?([ \t]*\[[ X-]\])").unwrap());
        if let Some(found) = buffer.text.get(point..).and_then(|rest| BOX.captures(rest)) {
            buffer.md = MatchData::from_captures(&found, point);
        }
        point = replace_match(buffer, origin, 1, "")?;
    } else if let Some(new_box) = &new_box {
        let counter = buffer.md.group(2).map(|(_, end)| end);
        point = match counter {
            Some(end) => end + 1,
            None => buffer.md.group(1).ok_or_else(|| Refusal::Failed(ARGUMENT_ERROR.to_string()))?.1,
        };
        if point > buffer.text.len() || !buffer.text.is_char_boundary(point) {
            return Err(Refusal::Failed("Args out of range".to_string()));
        }
        let inserted = if counter.is_some() { new_box.clone() } else { format!("{new_box} ") };
        insert(buffer, origin, point, &inserted, false);
        point += inserted.len();
    }
    // c. The indentation, of the line the point is on.
    if new_ind != old_ind as isize {
        let bol = line_start(&buffer.text, point.min(buffer.text.len()));
        let current = line(&buffer.text, bol);
        let blanks = current.len() - current.trim_start_matches([' ', '\t']).len();
        delete(buffer, origin, bol, bol + blanks);
        insert(buffer, origin, bol, &" ".repeat(new_ind.max(0) as usize), false);
    }
    Ok(())
}

// ----- commands on the item at point

/// A list edited by one of the commands below: its text and where the
/// cursor goes.
pub struct Edited {
    pub text: String,
    pub cursor: usize,
}

/// The structure of the list whose item starts the line at `at`, and that
/// item's index; `Not at an item` elsewhere.
fn item_here(text: &str, at: usize) -> Result<(usize, Struct, usize), Refusal> {
    if !at_item(text, at) {
        return Err(Refusal::NotHere("Not at an item".to_string()));
    }
    let pos = line_start(text, at.min(text.len()));
    let list = structure(text, pos);
    let index =
        list.iter().position(|item| item.pos == pos).ok_or_else(|| Refusal::NotHere("Not at an item".to_string()))?;
    Ok((pos, list, index))
}

fn failed(message: &str) -> Refusal {
    Refusal::Failed(message.to_string())
}

/// `org-list-struct-outdent`: the new parents of the items from `start`
/// (included) to `end` (excluded).
fn struct_outdent(
    start: usize,
    end: usize,
    list: &Struct,
    parents_of: &[Option<usize>],
) -> Result<Vec<Option<usize>>, Refusal> {
    let mut acc: Vec<(usize, Option<usize>)> = Vec::new();
    let mut out = Vec::with_capacity(list.len());
    for (item, parent) in parents_of.iter().copied().enumerate() {
        let pos = list[item].pos;
        out.push(if pos < start {
            parent
        } else if pos >= end {
            match parent.and_then(|parent| acc.iter().find(|(key, _)| *key == parent)) {
                Some((_, new)) => *new,
                None => parent,
            }
        } else {
            let Some(parent) = parent else { return Err(failed("Cannot outdent top-level items")) };
            if list[parent].pos >= start {
                acc.insert(0, (parent, Some(item)));
                Some(parent)
            } else {
                acc.insert(0, (parent, Some(item)));
                parents_of[parent]
            }
        });
    }
    Ok(out)
}

/// `org-list-struct-indent`: the new parents of the items from `start`
/// (included) to `end` (excluded), whose bullets change as
/// `org-list-demote-modify-bullet` says.
fn struct_indent(
    start: usize,
    end: usize,
    list: &mut Struct,
    parents_of: &[Option<usize>],
    prevs_of: &[Option<usize>],
    settings: &Settings,
) -> Result<Vec<Option<usize>>, Refusal> {
    let mut acc: Vec<(usize, Option<usize>)> = Vec::new();
    let mut out = Vec::with_capacity(list.len());
    for (item, parent) in parents_of.iter().copied().enumerate() {
        let pos = list[item].pos;
        out.push(if pos < start {
            parent
        } else if pos >= end {
            match parent.and_then(|parent| acc.iter().find(|(key, _)| *key == parent)) {
                Some((_, new)) => *new,
                None => parent,
            }
        } else {
            let kind = demoted_kind(&list[item].bullet);
            if let Some((_, new)) = settings.demote_bullets.iter().find(|(old, _)| *old == kind) {
                list[item].bullet = bullet_string(new, settings);
            }
            let cell = match prevs_of[item] {
                None if parent.is_none_or(|parent| list[parent].pos < start) => {
                    return Err(failed("Cannot indent the first item of a list"));
                }
                None => parent,
                Some(prev) if list[prev].pos < start => Some(prev),
                Some(prev) => acc.iter().find(|(key, _)| *key == prev).and_then(|(_, new)| *new),
            };
            acc.insert(0, (item, cell));
            cell
        });
    }
    Ok(out)
}

/// The kind of `bullet` that `org-list-demote-modify-bullet` is keyed by.
fn demoted_kind(bullet: &str) -> String {
    type Class = fn(&u8) -> bool;
    let bytes = bullet.as_bytes();
    let kinds: [(Class, u8, &str); 6] = [
        (u8::is_ascii_uppercase, b'.', "A."),
        (u8::is_ascii_uppercase, b')', "A)"),
        (u8::is_ascii_lowercase, b'.', "a."),
        (u8::is_ascii_lowercase, b')', "a)"),
        (u8::is_ascii_digit, b'.', "1."),
        (u8::is_ascii_digit, b')', "1)"),
    ];
    for (class, terminator, kind) in kinds {
        if bytes.windows(2).any(|pair| class(&pair[0]) && pair[1] == terminator) {
            return kind.to_string();
        }
    }
    bullet.trim_matches([' ', '\t', '\n', '\r']).to_string()
}

/// `org-list-indent-item-generic` from Org's meta keys on an item:
/// `M-<right>` (`org-indent-item`, the item alone), `M-S-<right>`
/// (`org-indent-item-tree`, with its children), and `M-<left>`,
/// `M-S-<left>` outdenting them (`change` below 0). On the first item of
/// a list the whole list moves by a column, with the `indent` rule of
/// `org-list-automatic-rules`. Otherwise the checkbox cookies are counted
/// again (`org-update-checkbox-count-maybe`).
pub fn indent_item(
    text: &str,
    at: usize,
    change: isize,
    subtree: bool,
    keywords: &org_syntax::TodoKeywords,
    tags_column: i64,
    settings: &Settings,
) -> Result<Edited, Refusal> {
    let (pos, mut list, index) = item_here(text, at)?;
    let parents_of = parents(&list);
    let prevs_of = prevs(&list);
    let top = list[0].pos;
    let special = top == pos && settings.indent_rule;
    if special && !subtree {
        return Err(failed("At first item: use S-M-<left/right> to move the whole list"));
    }
    let beg = pos;
    let end = if special {
        list.iter().map(|item| item.end).max().unwrap_or(pos)
    } else if !subtree {
        pos + 1
    } else {
        list[index].end
    };
    let mut buffer = Buffer { text: text.to_string(), point: at.min(text.len()), md: MatchData::default(), settings };
    let has_child = |list: &Struct, item: usize| list.get(item + 1).is_some_and(|next| list[item].ind < next.ind);
    if special {
        let offset: isize = if change < 0 { -1 } else { 1 };
        let top_ind = list[index].ind;
        if top_ind + offset < 0 {
            return Err(failed("Cannot outdent beyond margin"));
        }
        let old = list.clone();
        if top_ind + offset == 0 && list[index].bullet.contains('*') {
            list[index].bullet = bullet_string("-", settings);
        }
        for item in list.iter_mut() {
            item.ind += offset;
        }
        if fix_bul(&mut list, &prevs_of, settings) {
            buffer.md = MatchData::string();
        }
        apply_struct(&mut buffer, &list, &old)?;
        // Moving the whole list leaves the checkbox cookies alone.
        return Ok(Edited { cursor: buffer.point, text: buffer.text });
    } else {
        let last = list.iter().rposition(|item| item.pos < end).unwrap_or(index);
        if change < 0 && ((!subtree && has_child(&list, index)) || has_child(&list, last)) {
            return Err(failed("Cannot outdent an item without its children"));
        }
        let old = list.clone();
        let new_parents = if change < 0 {
            struct_outdent(beg, end, &list, &parents_of)?
        } else {
            struct_indent(beg, end, &mut list, &parents_of, &prevs_of, settings)?
        };
        write_struct(&mut buffer, &mut list, &new_parents, Some(&old))?;
    }
    if !settings.checkbox_rule {
        return Ok(Edited { cursor: buffer.point, text: buffer.text });
    }
    let (text, cursor) = crate::checkbox::update_counts_keeping(
        &buffer.text,
        keywords,
        buffer.point,
        tags_column,
        buffer.point,
        settings.checkbox_hierarchical,
    );
    Ok(Edited { text, cursor })
}

/// `org-move-item-down` (`M-<down>` on an item) and `org-move-item-up`:
/// the item and its children swapped with the next or previous one, the
/// cursor going with it, at its column. With `circular`
/// (`org-list-use-circular-motion`) the last item goes to the start of
/// its list, the first to its end.
pub fn move_item(text: &str, at: usize, down: bool, circular: bool, settings: &Settings) -> Result<Edited, Refusal> {
    let (item, mut list, index) = item_here(text, at)?;
    let column = text[item..at.min(text.len())].chars().count();
    let prevs_of = prevs(&list);
    let mut buffer = Buffer { text: text.to_string(), point: at.min(text.len()), md: MatchData::default(), settings };
    let next_of = |index: usize| prevs_of.iter().position(|prev| *prev == Some(index));
    if circular && ((down && next_of(index).is_none()) || (!down && prevs_of[index].is_none())) {
        return send_item(text, at, item, column, down, settings);
    }
    if down {
        let Some(next) = prevs_of.iter().position(|prev| *prev == Some(index)) else {
            return Err(failed("Cannot move this item further down"));
        };
        let next_pos = list[next].pos;
        swap_items(&mut buffer, item, next_pos, &mut list);
        let prevs_of = prevs(&list);
        let at = list.iter().position(|other| other.pos == item).expect("the item");
        let next = prevs_of.iter().position(|prev| *prev == Some(at)).expect("the moved item");
        buffer.point = list[next].pos;
    } else {
        let Some(prev) = prevs_of[index] else { return Err(failed("Cannot move this item further up")) };
        let prev_pos = list[prev].pos;
        let end_no_blank = end_before_blank(&buffer.text, list[index].end);
        swap_items(&mut buffer, prev_pos, item, &mut list);
        // Point was in the swapped text: it goes to its start.
        if buffer.point >= prev_pos && buffer.point <= end_no_blank {
            buffer.point = prev_pos;
        }
    }
    let parents_of = parents(&list);
    write_struct(&mut buffer, &mut list, &parents_of, None)?;
    // `org-move-to-column`
    let start = line_start(&buffer.text, buffer.point);
    let end = line_end(&buffer.text, start);
    let cursor = buffer.text[start..end].char_indices().nth(column).map_or(end, |(offset, _)| start + offset);
    Ok(Edited { text: buffer.text, cursor })
}

/// `org-cycle-list-bullet` (`S-<right>` on an item; `S-<left>` with
/// `previous`): the bullets of the item's sub-list through `-`, `+`, `*`
/// (not at the margin), `1.` and `1)` (not on a description list).
pub fn cycle_bullet(text: &str, at: usize, previous: bool, settings: &Settings) -> Result<Edited, Refusal> {
    let (pos, mut list, index) = item_here(text, at)?;
    let parents_of = parents(&list);
    let prevs_of = prevs(&list);
    let origin = at.min(text.len());
    let offset = origin as isize - (pos as isize + list[index].ind);
    let offset2 = offset - list[index].bullet.len() as isize;
    let mut first = index;
    while let Some(prev) = prevs_of[first] {
        first = prev;
    }
    let bullet = list[first].bullet.trim().to_string();
    let current = if bullet.contains('.') {
        "1.".to_string()
    } else if bullet.contains(')') {
        "1)".to_string()
    } else {
        bullet
    };
    // `org-at-item-description-p`: a tag of two characters or more, after
    // the counter, a checkbox counting as part of it.
    static COUNTER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\A\[@(?:start:)?[0-9]+\][ \t]*").unwrap());
    static DESCRIPTION: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\A\S.+[ \t]+::(?:[ \t]+|$)").unwrap());
    let after_bullet = line(text, pos).get(list[index].ind.max(0) as usize + list[index].bullet.len()..).unwrap_or("");
    let after_counter = COUNTER.find(after_bullet).map_or(after_bullet, |found| &after_bullet[found.end()..]);
    let description = DESCRIPTION.is_match(after_counter);
    let mut choices = vec!["-", "+"];
    if text[pos..].starts_with([' ', '\t']) {
        choices.push("*");
    }
    if !description {
        choices.extend(["1.", "1)"]);
    }
    let len = choices.len() as isize;
    let item_index = choices.iter().position(|choice| *choice == current).map_or(len, |found| found as isize);
    let new = choices[(item_index + if previous { -1 } else { 1 }).rem_euclid(len) as usize];
    let old = list.clone();
    list[first].bullet = bullet_string(new, settings);
    // `org-list-struct-apply-struct` runs from the start of the line, so
    // the spaces after a cursor in the bullet are not kept; the cursor
    // is placed again below, or else follows the edits as a marker.
    let point = if offset >= 0 { pos } else { origin };
    let mut buffer = Buffer { text: text.to_string(), point, md: MatchData::default(), settings };
    if fix_bul(&mut list, &prevs_of, settings) {
        buffer.md = MatchData::string();
    }
    fix_ind(&mut list, &parents_of, None, settings);
    apply_struct(&mut buffer, &list, &old)?;
    // The cursor keeps its place after the bullet, or before it.
    let origin = buffer.point;
    let start = line_start(&buffer.text, origin);
    let mut cursor = origin;
    if let Some(item) = structure(&buffer.text, start).into_iter().find(|item| item.pos == start) {
        if offset2 >= 0 {
            cursor = (start as isize + item.ind.max(0) + (item.bullet.len() as isize) + offset2) as usize;
        } else if offset >= 0 {
            cursor = (start as isize + item.ind.max(0) + offset) as usize;
        }
    }
    let cursor = cursor.min(buffer.text.len());
    Ok(Edited { text: buffer.text, cursor })
}

/// `org-cycle-list-bullet` with the bullet given (`1.`, `-`): the item at
/// `at` and its siblings take it. `marker` follows the edits.
pub(crate) fn set_bullet(
    text: &str,
    at: usize,
    new: &str,
    marker: usize,
    settings: &Settings,
) -> Result<(String, usize), Refusal> {
    let (_, mut list, index) = item_here(text, at)?;
    let parents_of = parents(&list);
    let prevs_of = prevs(&list);
    let mut first = index;
    while let Some(prev) = prevs_of[first] {
        first = prev;
    }
    let old = list.clone();
    list[first].bullet = bullet_string(new, settings);
    let mut buffer = Buffer { text: text.to_string(), point: marker, md: MatchData::default(), settings };
    if fix_bul(&mut list, &prevs_of, settings) {
        buffer.md = MatchData::string();
    }
    fix_ind(&mut list, &parents_of, None, settings);
    apply_struct(&mut buffer, &list, &old)?;
    Ok((buffer.text, buffer.point))
}

/// [`repair`] with the point at `at` and `marker` following the edits.
pub(crate) fn repair_with_marker(
    text: &str,
    at: usize,
    marker: usize,
    settings: &Settings,
) -> Result<(String, usize), Refusal> {
    let (_, mut list, _) = item_here(text, at)?;
    let parents_of = parents(&list);
    let mut buffer = Buffer { text: text.to_string(), point: marker, md: MatchData::default(), settings };
    write_struct(&mut buffer, &mut list, &parents_of, None)?;
    Ok((buffer.text, buffer.point))
}

/// What `C-c C-c` does on the line of an item (`org-ctrl-c-ctrl-c`), with
/// the structure org-element gives the list from `begin` (up to
/// `limit`): the item's checkbox toggled if it has one, and the list's
/// indentation, bullets, numbers and checkboxes put right. The text, the
/// cursor, and what to tell the user.
pub(crate) fn ctrl_c_ctrl_c(
    text: &str,
    at: usize,
    begin: usize,
    limit: usize,
    ordered: bool,
    settings: &Settings,
) -> Result<(String, usize, Option<String>), Refusal> {
    let pos = line_start(text, at.min(text.len()));
    let mut list = element_structure(text, begin, limit);
    let index =
        list.iter().position(|item| item.pos == pos).ok_or_else(|| Refusal::NotHere("Not at an item".to_string()))?;
    let old = list.clone();
    let parents_of = parents(&list);
    let prevs_of = prevs(&list);
    let checkbox = list[index].checkbox.clone();
    list[index].checkbox = match checkbox.as_deref() {
        None => None,
        Some("[X]") => Some("[ ]".to_string()),
        Some(_) => Some("[X]".to_string()),
    };
    let mut buffer = Buffer { text: text.to_string(), point: at.min(text.len()), md: MatchData::default(), settings };
    // `org-list-write-struct`, but bullets follow the items before them as
    // they were.
    fix_ind(&mut list, &parents_of, Some(2), settings);
    fix_item_end(&mut list);
    if fix_bul(&mut list, &prevs_of, settings) {
        buffer.md = MatchData::string();
    }
    fix_ind(&mut list, &parents_of, None, settings);
    let blocked = fix_box_ordered(&mut list, &parents_of, ordered);
    if checkbox.is_some() && list == old {
        let why = if checkbox.as_deref() == Some("[X]") { "all subitems checked" } else { "unchecked subitems" };
        return Err(failed(&format!("Cannot toggle this checkbox: {why}")));
    }
    apply_struct(&mut buffer, &list, &old)?;
    let message = blocked.map(|item| {
        let line = buffer.text[..item.min(buffer.text.len())].matches('\n').count() + 1;
        format!("Checkboxes were removed due to empty box at line {line}")
    });
    Ok((buffer.text, buffer.point, message))
}

/// What `C-c C-c` does at the start of a list (`org-ctrl-c-ctrl-c` on a
/// plain list, the cursor at `at`): the checkbox of its first item
/// toggled if it has one, and the list repaired (`org-list-write-struct`
/// on the structure org-element gives the list from `begin`).
pub(crate) fn ctrl_c_ctrl_c_list(
    text: &str,
    at: usize,
    begin: usize,
    limit: usize,
    settings: &Settings,
) -> Result<(String, usize, Option<String>), Refusal> {
    let pos = line_start(text, at.min(text.len()));
    let mut list = element_structure(text, begin, limit);
    let index =
        list.iter().position(|item| item.pos == pos).ok_or_else(|| Refusal::NotHere("Not at an item".to_string()))?;
    let old = list.clone();
    if let Some(checkbox) = &list[index].checkbox {
        list[index].checkbox = Some(if checkbox == "[X]" { "[ ]" } else { "[X]" }.to_string());
    }
    let parents_of = parents(&list);
    let mut buffer = Buffer { text: text.to_string(), point: at.min(text.len()), md: MatchData::default(), settings };
    write_struct(&mut buffer, &mut list, &parents_of, Some(&old))?;
    Ok((buffer.text, buffer.point, None))
}

/// Where a list to sort starts and ends, and where each item ends.
pub(crate) type SortBounds = (usize, usize, Vec<(usize, usize)>);

/// What `org-sort-list` sorts from the item at `at`: the list of that
/// item and its siblings, from the first to the end of the last
/// (`org-list-get-list-begin`, `org-list-get-list-end`), and where each
/// item of the structure ends before the blank lines after it
/// (`org-list-get-item-end-before-blank`).
pub(crate) fn sort_bounds(text: &str, at: usize) -> Result<SortBounds, Refusal> {
    let (_, list, index) = item_here(text, at)?;
    let prevs_of = prevs(&list);
    let mut first = index;
    while let Some(prev) = prevs_of[first] {
        first = prev;
    }
    let mut last = index;
    while let Some(next) = prevs_of.iter().position(|prev| *prev == Some(last)) {
        last = next;
    }
    let ends = list.iter().map(|item| (item.pos, end_before_blank(text, item.end))).collect();
    Ok((list[first].pos, list[last].end, ends))
}

/// `org-list-repair` with the cursor at `at` on an item: the list's
/// indentation, bullets and checkboxes put right.
pub(crate) fn repair(text: &str, at: usize, settings: &Settings) -> Result<Edited, Refusal> {
    let (_, mut list, _) = item_here(text, at)?;
    let parents_of = parents(&list);
    let mut buffer = Buffer { text: text.to_string(), point: at.min(text.len()), md: MatchData::default(), settings };
    write_struct(&mut buffer, &mut list, &parents_of, None)?;
    Ok(Edited { text: buffer.text, cursor: buffer.point })
}

/// `org-next-item` (`S-<down>` on an item) and `org-previous-item`
/// (`S-<up>`): the start of the next or previous item of the list; with
/// `circular` (`org-list-use-circular-motion`), from the last to the first
/// and back.
pub fn next_item(text: &str, at: usize, previous: bool, circular: bool) -> Result<usize, Refusal> {
    let item = in_item(text, at).ok_or_else(|| Refusal::NotHere("Not in an item".to_string()))?;
    let list = structure(text, item);
    let index = list.iter().position(|found| found.pos == item).ok_or_else(|| failed("Not in an item"))?;
    let prevs_of = prevs(&list);
    let next_of = |index: usize| prevs_of.iter().position(|prev| *prev == Some(index));
    let found = if previous { prevs_of[index] } else { next_of(index) };
    match found {
        Some(found) => Ok(list[found].pos),
        None if circular => {
            // `org-list-get-last-item` or `org-list-get-first-item`.
            let mut at = index;
            if previous {
                while let Some(next) = next_of(at) {
                    at = next;
                }
            } else {
                while let Some(prev) = prevs_of[at] {
                    at = prev;
                }
            }
            Ok(list[at].pos)
        }
        None => Err(failed(if previous { "On first item" } else { "On last item" })),
    }
}

/// `org-list-send-item` to `begin` (the last item moved down) or `end`
/// (the first moved up): the item swapped past each of its siblings, the
/// cursor going with it, at its column.
fn send_item(
    text: &str,
    _at: usize,
    item: usize,
    column: usize,
    to_begin: bool,
    settings: &Settings,
) -> Result<Edited, Refusal> {
    let mut current = text.to_string();
    let mut place = item;
    loop {
        let (here, list, index) = item_here(&current, place)?;
        let prevs_of = prevs(&list);
        let other = if to_begin { prevs_of[index] } else { prevs_of.iter().position(|prev| *prev == Some(index)) };
        let Some(other) = other else { break };
        let _ = other;
        let edited = move_item(&current, here, !to_begin, false, settings)?;
        current = edited.text;
        place = line_start(&current, edited.cursor);
    }
    let start = line_start(&current, place.min(current.len()));
    let end = line_end(&current, start);
    let cursor = current[start..end].char_indices().nth(column).map_or(end, |(offset, _)| start + offset);
    Ok(Edited { text: current, cursor })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_items() {
        let text = "* A\n- one\n  more\n- two\n";
        assert_eq!(in_item(text, 13), Some(4));
        assert_eq!(in_item(text, 19), Some(17));
        assert_eq!(in_item(text, 0), None);
        let list = structure(text, 4);
        assert_eq!(list.iter().map(|item| (item.pos, item.end)).collect::<Vec<_>>(), [(4, 17), (17, 23)]);
    }
}
