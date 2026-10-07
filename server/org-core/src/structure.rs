//! Structure editing on headlines: `org-do-promote` and `org-do-demote`
//! (`M-<left>`, `M-<right>`), `org-promote-subtree` and
//! `org-demote-subtree` (`M-S-<left>`, `M-S-<right>`), and
//! `org-move-subtree-up` and `org-move-subtree-down` (`M-<up>`,
//! `M-<down>`). The text is edited as Emacs edits the buffer, and the
//! cursor follows as Emacs's point does. Checked against Emacs in
//! `tests/structure`.

use std::sync::LazyLock;

use org_syntax::TodoKeywords;
use regex::Regex;

use crate::indent;
use crate::newline::Outcome;
use crate::todo::{headline_level, tags_padding};
use crate::Refusal;

pub struct Settings {
    /// `org-tags-column`
    pub tags_column: i64,
    /// `org-adapt-indentation` and what goes with it, for the text of an
    /// entry whose headline changes level (`org-fixup-indentation`).
    pub indent: indent::Settings,
}

/// Text and a point, edited as Emacs edits a buffer: the point moves as
/// a marker of `save-excursion` does.
struct Buffer {
    text: String,
    point: usize,
}

impl Buffer {
    /// `replace-match`: a point inside the replaced text goes to its
    /// start, one at its end or after moves with the text after it.
    fn replace(&mut self, start: usize, end: usize, new: &str) {
        if self.point >= end {
            self.point = self.point + new.len() - (end - start);
        } else if self.point > start {
            self.point = start;
        }
        self.text.replace_range(start..end, new);
    }

    /// `delete-region`
    fn delete(&mut self, start: usize, end: usize) {
        if self.point >= end {
            self.point -= end - start;
        } else if self.point > start {
            self.point = start;
        }
        self.text.replace_range(start..end, "");
    }

    /// `insert` at `at`, for a point that stays before what is inserted
    /// at its place.
    fn insert(&mut self, at: usize, what: &str) {
        if self.point > at {
            self.point += what.len();
        }
        self.text.insert_str(at, what);
    }
}

fn line_start(text: &str, at: usize) -> usize {
    text[..at].rfind('\n').map_or(0, |newline| newline + 1)
}

fn line_end(text: &str, at: usize) -> usize {
    text[at..].find('\n').map_or(text.len(), |newline| at + newline)
}

fn line(text: &str, start: usize) -> &str {
    &text[start..line_end(text, start)]
}

/// The start of the line after the one at `at`, or the end of the text.
fn next_line(text: &str, at: usize) -> usize {
    text[at..].find('\n').map_or(text.len(), |newline| at + newline + 1)
}

/// The level of the headline that starts at `at`, if one does.
fn level_at(text: &str, at: usize) -> Option<usize> {
    headline_level(line(text, at))
}

/// `outline-next-heading`: the start of the next headline after the line
/// at `at`, or the end of the text.
fn next_heading(text: &str, at: usize) -> usize {
    let mut at = next_line(text, at);
    while at < text.len() && level_at(text, at).is_none() {
        at = next_line(text, at);
    }
    at
}

/// `outline-previous-heading`: the start of the headline before the line
/// at `at`, if there is one.
fn previous_heading(text: &str, at: usize) -> Option<usize> {
    let mut at = line_start(text, at);
    while at > 0 {
        at = line_start(text, at - 1);
        if level_at(text, at).is_some() {
            return Some(at);
        }
    }
    None
}

/// The headline the cursor's line is, as `org-at-heading-p` sees it.
pub fn at_heading(text: &str, at: usize) -> bool {
    level_at(text, line_start(text, at.min(text.len()))).is_some()
}

/// Where the subtree of the headline at `heading` ends: at the next
/// headline of its level or above, or at the end of the text.
fn subtree_end(text: &str, heading: usize) -> usize {
    let level = level_at(text, heading).unwrap_or(1);
    let mut at = next_heading(text, heading);
    while at < text.len() && level_at(text, at).is_some_and(|found| found > level) {
        at = next_heading(text, at);
    }
    at
}

/// `org--align-tags-here` on the headline at `heading`, for a point
/// elsewhere (`org-promote` aligns from the start of the line).
fn align_tags(buffer: &mut Buffer, heading: usize, tags_column: i64) {
    let end = line_end(&buffer.text, heading);
    let Some((blanks, tags, padding)) = tags_padding(&buffer.text[heading..end], tags_column) else { return };
    buffer.delete(heading + blanks, heading + tags);
    buffer.insert(heading + blanks, &padding);
}

/// `org-promote` (`change` -1) or `org-demote` (1) of the headline at
/// `heading`: its stars, its tags aligned again, and with
/// `org-adapt-indentation` its text (`org-fixup-indentation`).
fn change_level(buffer: &mut Buffer, heading: usize, change: isize, settings: &Settings) -> Result<(), Refusal> {
    let level = level_at(&buffer.text, heading).ok_or("Not at a headline")?;
    if change < 0 && level == 1 {
        return Err(Refusal::Failed("Cannot promote to level 0.  UNDO to recover if necessary".to_string()));
    }
    let new = (level as isize + change) as usize;
    buffer.replace(heading, heading + level + 1, &format!("{} ", "*".repeat(new)));
    align_tags(buffer, heading, settings.tags_column);
    if settings.indent.adapt != indent::Adapt::Off {
        let end = next_heading(&buffer.text, heading);
        let entry = buffer.text[heading..end].to_string();
        let fixed = indent::fixup_entry(&entry, change, &settings.indent);
        if fixed != entry {
            // The headline stays as it is; only the lines after it change.
            let body = next_line(&entry, 0).min(entry.len());
            let fixed_body = next_line(&fixed, 0).min(fixed.len());
            buffer.replace(heading + body, end, &fixed[fixed_body..]);
        }
    }
    Ok(())
}

/// `org-fix-position-after-promote`: a cursor right after the stars or
/// the TODO keyword steps over the space after them, or gets one.
fn fix_position(buffer: &mut Buffer, keywords: &TodoKeywords) {
    let start = line_start(&buffer.text, buffer.point);
    let current = line(&buffer.text, start);
    let Some(stars) = headline_level(current) else { return };
    let mut ends = vec![start + stars];
    let rest = &current[stars..];
    let blanks = rest.len() - rest.trim_start_matches(' ').len();
    if blanks > 0 {
        let word = rest[blanks..].split([' ', '\t']).next().unwrap_or("");
        if keywords.names().any(|keyword| keyword == word) {
            let after = &rest[blanks + word.len()..];
            // The keyword is a group only when what follows matches too.
            if after.is_empty() || after.starts_with(' ') || after.trim_start_matches([' ', '\t']).is_empty() {
                ends.push(start + stars + blanks + word.len());
            }
        }
    }
    if !ends.contains(&buffer.point) {
        return;
    }
    let point = buffer.point;
    if point == buffer.text.len() || buffer.text[point..].starts_with('\n') {
        buffer.insert(point, " ");
        buffer.point = point + 1;
    } else if buffer.text[point..].starts_with(' ') {
        buffer.point += 1;
    }
}

/// `M-<left>` (`org-do-promote`), `M-<right>` (`org-do-demote`), and with
/// `subtree` `M-S-<left>` (`org-promote-subtree`) and `M-S-<right>`
/// (`org-demote-subtree`), on the headline at `at`.
pub fn change_levels(
    text: &str,
    at: usize,
    change: isize,
    subtree: bool,
    keywords: &TodoKeywords,
    settings: &Settings,
) -> Result<Outcome, Refusal> {
    let heading = line_start(text, at);
    let level = level_at(text, heading).ok_or_else(|| Refusal::NotHere("Not at a headline".to_string()))?;
    let mut buffer = Buffer { text: text.to_string(), point: at };
    change_level(&mut buffer, heading, change, settings)?;
    if subtree {
        // `org-map-tree`: the headlines below, down to one of the old level
        // or above.
        let mut next = next_heading(&buffer.text, heading);
        while next < buffer.text.len() && level_at(&buffer.text, next).is_some_and(|found| found > level) {
            change_level(&mut buffer, next, change, settings)?;
            next = next_heading(&buffer.text, next);
        }
    }
    fix_position(&mut buffer, keywords);
    Ok(Outcome { text: buffer.text, cursor: buffer.point })
}

/// `org-get-next-sibling` from the headline at `at`.
fn next_sibling(text: &str, at: usize) -> Option<usize> {
    let level = level_at(text, at)?;
    let mut next = next_heading(text, at);
    while next < text.len() && level_at(text, next).is_some_and(|found| found > level) {
        next = next_heading(text, next);
    }
    (next < text.len() && level_at(text, next).is_some_and(|found| found >= level)).then_some(next)
}

/// `org-get-previous-sibling` from the headline at `at`.
fn previous_sibling(text: &str, at: usize) -> Option<usize> {
    let level = level_at(text, at)?;
    let mut previous = previous_heading(text, at)?;
    // Deeper headlines are passed, up to the start of the text, where
    // `outline-previous-heading` stops whatever is there.
    while previous > 0 && level_at(text, previous).is_some_and(|found| found > level) {
        previous = previous_heading(text, previous).unwrap_or(0);
    }
    level_at(text, previous).is_some_and(|found| found >= level).then_some(previous)
}

/// The column of `at` in its line, as `current-column` counts it.
fn column(text: &str, at: usize) -> usize {
    crate::text::width(&text[line_start(text, at)..at])
}

/// `move-to-column`: the place in the line at `start` at `column`, or its
/// end.
fn to_column(text: &str, start: usize, column: usize) -> usize {
    let mut width = 0;
    for (index, c) in line(text, start).char_indices() {
        if width >= column {
            return start + index;
        }
        width += crate::text::char_width(c);
    }
    line_end(text, start)
}

/// `M-<down>` (`org-move-subtree-down`) with `count` 1 or more, `M-<up>`
/// (`org-move-subtree-up`) with -1 or less: the subtree at `at` past as
/// many of its siblings.
pub fn move_subtree(text: &str, at: usize, count: isize) -> Result<Outcome, Refusal> {
    let beg = line_start(text, at);
    if level_at(text, beg).is_none() {
        return Err(Refusal::NotHere("Not at a headline".to_string()));
    }
    let col = column(text, at);
    let end = subtree_end(text, beg);
    let mut buffer = Buffer { text: text.to_string(), point: beg };
    let mut place = beg;
    for _ in 0..count.unsigned_abs() {
        let found = if count > 0 { next_sibling(&buffer.text, place) } else { previous_sibling(&buffer.text, place) };
        place = found.ok_or_else(|| Refusal::Failed("Cannot move past superior level or buffer limit".to_string()))?;
    }
    buffer.point = place;
    if count > 0 {
        // Over the sibling's subtree; the text ends in a newline there
        // (`org-back-over-empty-lines` in `save-excursion`).
        buffer.point = subtree_end(&buffer.text, place);
        let mut back = buffer.text[..buffer.point].trim_end_matches([' ', '\t', '\n', '\r']).len();
        back = next_line(&buffer.text, back).min(buffer.point);
        let at_line_start = back == 0 || buffer.text[..back].ends_with('\n');
        if !at_line_start {
            buffer.insert(back, "\n");
        }
    }
    let tree = buffer.text[beg..end].to_string();
    buffer.delete(beg, end);
    if buffer.point > 0 && !buffer.text[..buffer.point].ends_with('\n') && buffer.text[buffer.point..].starts_with('\n')
    {
        buffer.point += 1;
    }
    let insert_at = buffer.point;
    buffer.text.insert_str(insert_at, &tree);
    buffer.point = insert_at + tree.len();
    if buffer.point > 0 && !buffer.text[..buffer.point].ends_with('\n') {
        buffer.insert(buffer.point, "\n");
        buffer.point += 1;
    }
    // `org-skip-whitespace`, then the old column.
    let mut cursor = insert_at;
    cursor += buffer.text[cursor..].len() - buffer.text[cursor..].trim_start_matches([' ', '\t', '\n']).len();
    let cursor = to_column(&buffer.text, line_start(&buffer.text, cursor), col);
    Ok(Outcome { text: buffer.text, cursor })
}

/// `M-<right>` on the first or last line of a drawer (`org-at-drawer-p`
/// and `org-indent-drawer`) or the first line of a block
/// (`org-at-block-p` and `org-indent-block`): the element's bytes. A line
/// that looks like a drawer's but is none is an error, as in Org.
pub fn drawer_or_block_at(
    root: &org_syntax::SyntaxNode,
    text: &str,
    offset: usize,
) -> Result<Option<(usize, usize)>, Refusal> {
    use org_syntax::SyntaxKind::{BLOCK, DRAWER, DYNAMIC_BLOCK, PROPERTY_DRAWER};
    static DRAWER_LINE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[ \t]*:[\w-]+:[ \t]*$").expect("valid"));
    static BLOCK_LINE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^[ \t]*#\+begin_?\S").expect("valid"));
    let start = text[..offset].rfind('\n').map_or(0, |at| at + 1);
    let end = text[start..].find('\n').map_or(text.len(), |at| start + at);
    let line = &text[start..end];
    let drawer = DRAWER_LINE.is_match(line);
    let block = BLOCK_LINE.is_match(line);
    if !drawer && !block {
        return Ok(None);
    }
    let at = org_syntax::TextSize::new(start as u32);
    let element = root.token_at_offset(at).right_biased().and_then(|token| {
        token.parent_ancestors().find(|node| matches!(node.kind(), DRAWER | PROPERTY_DRAWER | BLOCK | DYNAMIC_BLOCK))
    });
    let Some(element) = element else {
        return if drawer { Err(Refusal::Failed("Not at a drawer".to_string())) } else { Ok(None) };
    };
    let range = element.text_range();
    let (first, last) = (usize::from(range.start()), usize::from(range.end()));
    let first_line = first <= start && text[first..start].trim().is_empty();
    let is_drawer = matches!(element.kind(), DRAWER | PROPERTY_DRAWER);
    if drawer && is_drawer || block && !is_drawer && first_line {
        return Ok(Some((first, last)));
    }
    if drawer {
        return Err(Refusal::Failed("Not at a drawer".to_string()));
    }
    Ok(None)
}

/// `M-<right>` on a drawer or a block (`org-indent-drawer`,
/// `org-indent-block`): the element indented as `org-indent-region` does
/// it, and what Org says. None elsewhere.
pub fn indent_element(
    text: &str,
    at: usize,
    root: &org_syntax::SyntaxNode,
    settings: &indent::Settings,
) -> Result<Option<(Outcome, &'static str)>, Refusal> {
    let Some((start, end)) = drawer_or_block_at(root, text, at)? else { return Ok(None) };
    let drawer = text[start..].trim_start_matches([' ', '\t']).starts_with(':');
    let mut buffer = Buffer { text: text.to_string(), point: at };
    let mut edits = indent::indent_region(text, start, end, settings);
    // From the last, so that the places of the others hold.
    edits.sort_by_key(|edit| std::cmp::Reverse(edit.start));
    for edit in edits {
        // Indentation added where there was none is inserted at the
        // cursor's place, which stays before it, as with `indent-to`.
        if edit.start == edit.end {
            buffer.insert(edit.start, &edit.text);
        } else {
            buffer.replace(edit.start, edit.end, &edit.text);
        }
    }
    let message = if drawer { "Drawer at point indented" } else { "Block at point indented" };
    Ok(Some((Outcome { text: buffer.text, cursor: buffer.point }, message)))
}

// ----- cut, copy and paste

/// `org-forward-heading-same-level` with `invisible-ok`, `count` times
/// from the headline at `heading`: the last headline of its level found
/// before one of a higher level.
fn forward_same_level(text: &str, heading: usize, count: usize) -> usize {
    let Some(level) = level_at(text, heading) else { return heading };
    let mut result = heading;
    let mut at = heading;
    let mut count = count;
    while count > 0 {
        at = next_heading(text, at);
        let Some(found) = level_at(text, at).filter(|_| at < text.len()) else { break };
        if found < level {
            break;
        }
        if found == level {
            count -= 1;
            result = at;
        }
    }
    result
}

/// `C-c C-x M-w` (`org-copy-subtree`) and, with `cut`, `C-c C-x C-w`
/// (`org-cut-subtree`): the subtree the cursor is in and `count - 1`
/// siblings after it. Returns the text after it, the cursor, and what was
/// copied; and what Org says.
pub fn copy_subtree(text: &str, at: usize, count: usize, cut: bool) -> Result<(Outcome, String, String), Refusal> {
    let mut beg = line_start(text, at.min(text.len()));
    while level_at(text, beg).is_none() {
        if beg == 0 {
            return Err(Refusal::NotHere("Before first heading".to_string()));
        }
        beg = line_start(text, beg - 1);
    }
    let last = forward_same_level(text, beg, count.max(1) - 1);
    let end = subtree_end(text, last);
    let copied = text[beg..end].to_string();
    let mut buffer = Buffer { text: text.to_string(), point: at };
    if cut {
        buffer.delete(beg, end);
    }
    let message =
        format!("{}: Subtree(s) with {} characters", if cut { "Cut" } else { "Copied" }, copied.chars().count());
    Ok((Outcome { text: buffer.text, cursor: buffer.point }, copied, message))
}

/// `org-kill-is-subtree-p`: whether `tree` starts with a headline, after
/// blank lines, and has none of a higher level after it.
pub fn is_subtree(tree: &str) -> bool {
    let skipped = tree.len() - tree.trim_start_matches([' ', '\t', '\n', '\r']).len();
    // The blanks skipped end with a newline, or there are none.
    if skipped > 0 && !tree[..skipped].ends_with('\n') {
        return false;
    }
    let Some(first) = level_at(tree, skipped) else { return false };
    let mut at = skipped;
    loop {
        at = next_heading(tree, at);
        if at >= tree.len() {
            return true;
        }
        if level_at(tree, at).is_some_and(|level| level < first) {
            return false;
        }
    }
}

/// `C-c C-x C-y` (`org-paste-subtree`) of `tree` at `at`, with a level of
/// its own when `level` is given (a numeric prefix): at the start of a
/// headline before it, else before the next headline, its levels shifted
/// to the deeper of the headlines around it, or to the level of the stars
/// of an otherwise empty headline the cursor is on. The cursor goes to
/// the start of what was pasted. Returns what Org says too.
pub fn paste_subtree(
    text: &str,
    at: usize,
    tree: &str,
    level: Option<usize>,
    settings: &Settings,
) -> Result<(Outcome, String), Refusal> {
    if !is_subtree(tree) {
        return Err(Refusal::Failed("The clipboard is not a (set of) tree(s)".to_string()));
    }
    let at = at.min(text.len());
    let bol = line_start(text, at);
    let current = line(text, bol);
    let old_level = {
        let mut start = 0;
        loop {
            if let Some(found) = level_at(tree, start) {
                break found as isize;
            }
            if start >= tree.len() {
                break -1;
            }
            start = next_line(tree, start);
            if start >= tree.len() {
                break -1;
            }
        }
    };
    // An empty headline of stars the cursor is after says the level.
    let indicator = level.is_none()
        && !current.is_empty()
        && current.trim_end_matches([' ', '\t']).bytes().all(|b| b == b'*')
        && !text[at..].starts_with('*');
    let force = if indicator {
        Some(current.bytes().take_while(|b| *b == b'*').count())
    } else if let Some(level) = level {
        Some(level)
    } else if at == bol && level_at(text, bol).is_some() {
        level_at(text, bol)
    } else {
        None
    };
    let previous = if level_at(text, bol).is_some() {
        level_at(text, bol)
    } else {
        previous_heading(text, bol).and_then(|heading| level_at(text, heading))
    }
    .unwrap_or(1);
    // `org-next-visible-heading` stops at the end of the text, on the last
    // line when no newline ends it.
    let next = {
        let found = next_heading(text, bol);
        if found < text.len() {
            level_at(text, found)
        } else {
            level_at(text, line_start(text, found))
        }
    }
    .unwrap_or(1);
    let new_level = force.unwrap_or(previous.max(next)) as isize;
    let shift = if old_level == -1 || old_level == new_level { 0 } else { new_level - old_level };
    let mut buffer = Buffer { text: text.to_string(), point: at };
    if indicator {
        let end = next_line(&buffer.text, bol);
        buffer.delete(bol, end);
    }
    let point = buffer.point;
    let here = point == line_start(&buffer.text, point) && level_at(&buffer.text, point).is_some();
    if !here {
        buffer.point = next_heading(&buffer.text, line_start(&buffer.text, point));
        if buffer.point > 0 && buffer.point == buffer.text.len() && !buffer.text.ends_with('\n') {
            let end = buffer.text.len();
            buffer.text.push('\n');
            buffer.point = end + 1;
        }
    }
    let start = buffer.point;
    let mut pasted = tree.to_string();
    if !pasted.ends_with('\n') {
        pasted.push('\n');
    }
    buffer.text.insert_str(start, &pasted);
    let mut end = start + pasted.len();
    let first = start + (pasted.len() - pasted.trim_start_matches([' ', '\t', '\n', '\r']).len());
    // `org-map-region` of `org-promote` or `org-demote`, a level at a time,
    // over the headlines pasted.
    let step = shift.signum();
    for _ in 0..shift.unsigned_abs() {
        let mut heading = first;
        while heading < end {
            if level_at(&buffer.text, heading).is_some() {
                let before = buffer.text.len();
                change_level_within(&mut buffer, heading, step, end, settings)?;
                end = end + buffer.text.len() - before;
            }
            heading = next_line(&buffer.text, heading);
        }
    }
    let message = format!("Clipboard pasted as level {new_level} subtree");
    Ok((Outcome { text: buffer.text, cursor: first }, message))
}

/// `change_level` within text that ends at `limit` (the narrowing of
/// `org-paste-subtree`): the text of the last entry ends there.
fn change_level_within(
    buffer: &mut Buffer,
    heading: usize,
    change: isize,
    limit: usize,
    settings: &Settings,
) -> Result<(), Refusal> {
    let rest = buffer.text.split_off(limit);
    let result = change_level(buffer, heading, change, settings);
    buffer.text.push_str(&rest);
    result
}
