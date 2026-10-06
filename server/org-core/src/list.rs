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
    pub ind: usize,
    /// With the blanks after it.
    pub bullet: String,
    pub counter: Option<String>,
    pub checkbox: Option<String>,
    pub tag: Option<String>,
    pub end: usize,
}

pub(crate) type Struct = Vec<Item>;

// ----- lines

pub(crate) fn line_start(text: &str, at: usize) -> usize {
    text[..at.min(text.len())].rfind('\n').map_or(0, |index| index + 1)
}

pub(crate) fn line_end(text: &str, at: usize) -> usize {
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
        ind,
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
            if items_down.last().is_some_and(|last| ind <= last.ind) {
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
    assoc_end(&mut all, &end_list);
    all
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
fn assoc_end(items: &mut [Item], end_list: &[(usize, usize)]) {
    let mut endings: &[(usize, usize)] = end_list;
    for item in items.iter_mut() {
        while endings.first().is_some_and(|(_, end)| *end <= item.pos) {
            endings = &endings[1..];
        }
        item.end = endings.iter().find(|(ind, _)| *ind <= item.ind).map_or(0, |(_, end)| *end);
    }
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
    let mut ind_to_ori: Vec<(usize, Option<usize>)> = vec![(list[0].ind, None)];
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

/// `org-list-bullet-string`: the bullet with one space after it.
fn bullet_string(bullet: &str) -> String {
    let trimmed = bullet.trim_end_matches([' ', '\t']);
    if trimmed.is_empty() {
        return bullet.to_string();
    }
    format!("{trimmed} ")
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

/// `org-insert-item` at byte `pos`, with a checkbox when `checkbox`. None
/// when `pos` is not in an item.
pub(crate) fn insert_item(
    text: &str,
    pos: usize,
    checkbox: bool,
    keywords: &org_syntax::TodoKeywords,
    tags_column: i64,
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
    let mut buffer = Buffer { text: text.to_string(), point: pos, md: MatchData::default() };
    let mut list = list;
    insert_in_struct(&mut buffer, pos, &mut list, checkbox, desc);
    let parents_of = parents(&list);
    write_struct(&mut buffer, &mut list, &parents_of, None)?;
    let mut text = buffer.text;
    let mut point = buffer.point;
    if checkbox {
        // The cookies change, not the lines: the cursor stays on its line.
        let line_number = text[..point].matches('\n').count();
        text = crate::checkbox::update_counts(&text, keywords, point, tags_column);
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
struct Buffer {
    text: String,
    point: usize,
    md: MatchData,
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
fn insert_in_struct(buffer: &mut Buffer, pos: usize, list: &mut Struct, checkbox: bool, after_bullet: Option<&str>) {
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
            pos <= item + limit
        }
        None => true,
    };
    let prevs_of = prevs(list);
    let blank_nb = separating_blank_lines(&text, pos, list, item_index, &prevs_of, &mut buffer.md);
    let ind = list[item_index].ind;
    let bullet = bullet_string(&list[item_index].bullet);
    let box_text = if checkbox { "[ ] " } else { "" };
    let mut text = text;
    let mut pos = pos;
    // The text after the cursor goes to the new item.
    let text_cut = if !beforep {
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
        } else if index == item_index && !beforep {
            other.pos = p + item_size;
            other.end = shift(other.end, size_offset);
        } else if !beforep && p >= pos && p <= item_end_no_blank {
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
        ind,
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
    fix_ind(list, parents_of, Some(2));
    fix_item_end(list);
    let prevs_of = prevs(list);
    if fix_bul(list, &prevs_of) {
        buffer.md = MatchData::string();
    }
    fix_ind(list, parents_of, None);
    fix_box(list, parents_of);
    apply_struct(buffer, list, &old)
}

/// `org-list-struct-fix-ind`, with `org-list-indent-offset` at 0.
fn fix_ind(list: &mut Struct, parents_of: &[Option<usize>], bullet_size: Option<usize>) {
    let top_ind = list[0].ind;
    for index in 1..list.len() {
        list[index].ind = match parents_of[index] {
            Some(parent) => bullet_size.unwrap_or(list[parent].bullet.len()) + list[parent].ind,
            None => top_ind,
        };
    }
}

/// `org-list-struct-fix-item-end`
fn fix_item_end(list: &mut Struct) {
    let mut end_list: Vec<(usize, usize)> = Vec::new();
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
fn fix_bul(list: &mut Struct, prevs_of: &[Option<usize>]) -> bool {
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
        list[index].bullet = bullet_string(&new);
    }
    matched
}

/// `org-list-struct-fix-box`, not ordered.
fn fix_box(list: &mut Struct, parents_of: &[Option<usize>]) {
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
}

/// `org-list-struct-apply-struct`: the differences between `old` and
/// `list` made in the text.
fn apply_struct(buffer: &mut Buffer, list: &Struct, old: &Struct) -> Result<(), Refusal> {
    let mut origin = buffer.point;
    let find = |list: &Struct, pos: usize| list.iter().find(|item| item.pos == pos).cloned();
    // 1. The shift of each item, and the ends that are not items.
    let mut itm_shift: Vec<(usize, isize, usize)> = Vec::new();
    let mut end_list: Vec<(usize, Option<usize>)> = Vec::new();
    let mut acc_end: Vec<(usize, usize)> = Vec::new();
    for e in old {
        let new = find(list, e.pos).unwrap_or_else(|| e.clone());
        let shift = (new.ind + new.bullet.len()) as isize - (e.ind + e.bullet.len()) as isize;
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
    let mut slices: Vec<(usize, usize, isize, Option<usize>)> = Vec::new();
    for pair in all_ends.windows(2) {
        let (up, down) = (pair[0], pair[1]);
        let itemp = list.iter().any(|item| item.pos == up);
        let delta = if itemp {
            itm_shift.iter().find(|(pos, _, _)| *pos == up).map_or(0, |(_, shift, _)| *shift)
        } else {
            let child = acc_end.iter().find(|(end, _)| *end == up).map(|(_, item)| *item);
            let ind = child.and_then(|child| find(list, child)).map_or(0, |item| item.ind) as isize;
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
    let mut at = up;
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
    delete(buffer, origin, start, end);
    insert(buffer, origin, start, new, false);
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
fn shift_body(buffer: &mut Buffer, origin: &mut usize, end: usize, beg: usize, delta: isize, ind: Option<usize>) {
    static ITEM_AT: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?m)\A([ \t]*([-+]|(([0-9]+)[.)]))|[ \t]+\*)([ \t]+|$)").unwrap());
    let trimmed = buffer.text[..end.min(buffer.text.len())].trim_end_matches([' ', '\r', '\t', '\n']).len();
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
            let target = (indentation(&current) as isize + delta).max(ind.map_or(-1, |ind| ind as isize + 1)).max(0);
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
    let new_bullet = bullet_string(&new_item.bullet);
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
    if new_ind != old_ind {
        let bol = line_start(&buffer.text, point.min(buffer.text.len()));
        let current = line(&buffer.text, bol);
        let blanks = current.len() - current.trim_start_matches([' ', '\t']).len();
        delete(buffer, origin, bol, bol + blanks);
        insert(buffer, origin, bol, &" ".repeat(new_ind), false);
    }
    Ok(())
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
