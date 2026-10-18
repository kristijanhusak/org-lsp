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
    /// `org-allow-promoting-top-level-subtree`: promoting a headline of
    /// level 1 makes it a comment line, `# ` in place of `* `.
    pub allow_promoting_top_level: bool,
    /// `org-odd-levels-only`: headlines go two stars at a time.
    pub odd_levels: bool,
}

/// `org-odd-levels-only` as the options set it, for the server's life.
static ODD_LEVELS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Set `org-odd-levels-only` for the process.
pub fn set_odd_levels_default(odd: bool) {
    ODD_LEVELS.store(odd, std::sync::atomic::Ordering::Relaxed);
}

/// `org-odd-levels-only` as the options set it.
pub fn odd_levels_default() -> bool {
    ODD_LEVELS.load(std::sync::atomic::Ordering::Relaxed)
}

/// `org-odd-levels-only` in `text`: its `#+STARTUP:` words `odd` and
/// `oddeven`, the last one counting, else `default`.
pub fn odd_levels(text: &str, default: bool) -> bool {
    let mut odd = default;
    for word in crate::text::startup_words(text) {
        match word.to_ascii_lowercase().as_str() {
            "odd" => odd = true,
            "oddeven" => odd = false,
            _ => {}
        }
    }
    odd
}

/// `org-odd-levels-only` in `text`, else as the options set it.
pub fn odd_levels_in(text: &str) -> bool {
    odd_levels(text, odd_levels_default())
}

/// `org-get-valid-level`: the level `change` levels from `level`, with
/// `org-odd-levels-only` an odd one, two stars a level.
pub fn valid_level(level: usize, change: isize, odd: bool) -> usize {
    let level = level as isize;
    let valid = if !odd {
        level + change
    } else if change == 0 {
        1 + 2 * (level / 2)
    } else if change > 0 {
        1 + 2 * ((level - 1 + 2 * change) / 2)
    } else {
        1 + 2 * ((level + 2 * change) / 2)
    };
    valid.max(1) as usize
}

/// `org-reduced-level`: the level of a headline counted as
/// `org-odd-levels-only` counts it, `***` the second.
pub fn reduced_level(level: usize, odd: bool) -> usize {
    if odd {
        1 + level / 2
    } else {
        level
    }
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

pub(crate) fn line_start(text: &str, at: usize) -> usize {
    text[..at].rfind('\n').map_or(0, |newline| newline + 1)
}

pub(crate) fn line_end(text: &str, at: usize) -> usize {
    text[at..].find('\n').map_or(text.len(), |newline| at + newline)
}

pub(crate) fn line(text: &str, start: usize) -> &str {
    &text[start..line_end(text, start)]
}

/// The start of the line after the one at `at`, or the end of the text.
pub(crate) fn next_line(text: &str, at: usize) -> usize {
    text[at..].find('\n').map_or(text.len(), |newline| at + newline + 1)
}

/// The level of the headline that starts at `at`, if one does.
pub(crate) fn level_at(text: &str, at: usize) -> Option<usize> {
    headline_level(line(text, at))
}

/// `outline-next-heading`: the start of the next headline after the line
/// at `at`, or the end of the text.
pub(crate) fn next_heading(text: &str, at: usize) -> usize {
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
pub(crate) fn subtree_end(text: &str, heading: usize) -> usize {
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
    if change < 0 && level == 1 && settings.allow_promoting_top_level {
        buffer.replace(heading, heading + 2, "# ");
        return Ok(());
    }
    if change < 0 && level == 1 {
        return Err(Refusal::Failed("Cannot promote to level 0.  UNDO to recover if necessary".to_string()));
    }
    let new = valid_level(level, change, settings.odd_levels);
    // `org-fixup-indentation` by as many columns as the stars changed.
    let change = new as isize - level as isize;
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

/// `org-convert-to-odd-levels` and, with `odd` false,
/// `org-convert-to-oddeven-levels`: each headline below the top level
/// demoted to `2n-1` stars, or promoted back to `n`, one step at a time as
/// `org-demote` and `org-promote` go with `org-odd-levels-only` off. The
/// way back refuses a file with a headline of an even number of stars.
pub fn convert_levels(text: &str, at: usize, odd: bool, settings: &Settings) -> Result<Outcome, Refusal> {
    let headline = |line: &str| {
        let stars = line.bytes().take_while(|b| *b == b'*').count();
        (stars >= 2 && line[stars..].starts_with(' ')).then_some(stars)
    };
    let lines = || text.split_inclusive('\n').map(|line| line.trim_end_matches('\n'));
    if !odd && lines().filter_map(headline).any(|stars| stars % 2 == 0) {
        return Err(Refusal::Failed("Not all levels are odd in this file.  Conversion not possible".to_string()));
    }
    let settings = Settings {
        tags_column: settings.tags_column,
        indent: settings.indent.clone(),
        allow_promoting_top_level: settings.allow_promoting_top_level,
        odd_levels: false,
    };
    let mut buffer = Buffer { text: text.to_string(), point: at };
    let mut start = 0;
    while start < buffer.text.len() {
        if let Some(stars) = headline(line(&buffer.text, start)) {
            let (steps, change) = if odd { (stars - 1, 1) } else { (stars / 2, -1) };
            for _ in 0..steps {
                change_level(&mut buffer, start, change, &settings)?;
            }
        }
        start = next_line(&buffer.text, start);
    }
    Ok(Outcome { text: buffer.text, cursor: buffer.point })
}

/// `org-cycle-level`: TAB at the end of a new, empty headline (stars, and
/// perhaps a TODO keyword) makes it a child of the headline before, then
/// its parent's sibling and further up, then a sibling again. None
/// elsewhere.
pub fn cycle_level(text: &str, at: usize, keywords: &TodoKeywords, settings: &Settings) -> Option<Outcome> {
    let start = line_start(text, at);
    let end = line_end(text, at);
    // `org-point-at-end-of-empty-headline`
    if !text[at..end].trim_matches([' ', '\t']).is_empty() || !empty_headline(&text[start..end], keywords) {
        return None;
    }
    let current = text[start..].bytes().take_while(|b| *b == b'*').count();
    // `org-get-previous-line-level`: that of the headline the line before
    // is in, 0 for none.
    let previous = if start == 0 {
        0
    } else {
        let mut line = line_start(text, start - 1);
        loop {
            if let Some(level) = level_at(text, line) {
                break level;
            }
            if line == 0 {
                break 0;
            }
            line = line_start(text, line - 1);
        }
    };
    // With `org-adapt-indentation` off, as `org-cycle-level` binds it.
    let settings = Settings {
        tags_column: settings.tags_column,
        indent: indent::Settings { adapt: indent::Adapt::Off, ..settings.indent.clone() },
        allow_promoting_top_level: false,
        odd_levels: settings.odd_levels,
    };
    // `org-level-increment`: two stars a level with `org-odd-levels-only`.
    let increment = if settings.odd_levels { 2 } else { 1 };
    let steps: isize = if previous == 0 {
        -((current as isize - 1) / increment)
    } else if previous == current {
        1
    } else if previous == 1 {
        -((current as isize - 1) / increment)
    } else if current == 1 {
        (previous as isize - 1) / increment
    } else if current < previous {
        -1
    } else {
        -(1 + (current - previous) as isize / increment)
    };
    let mut outcome = Outcome { text: text.to_string(), cursor: at };
    for _ in 0..steps.unsigned_abs() {
        outcome = change_levels(&outcome.text, outcome.cursor, steps.signum(), false, keywords, &settings).ok()?;
    }
    Some(outcome)
}

/// `org-todo-line-regexp` matched with an empty title: the stars, perhaps
/// a TODO keyword, and blanks.
fn empty_headline(line: &str, keywords: &TodoKeywords) -> bool {
    let names: Vec<String> = keywords.names().map(regex::escape).collect();
    let keyword = if names.is_empty() { String::new() } else { format!("(?: +(?:{}))?", names.join("|")) };
    let pattern = format!(r"^\*+{keyword}(?: +(.*?))?[ \t]*$");
    Regex::new(&pattern)
        .ok()
        .and_then(|re| re.captures(line))
        .is_some_and(|found| found.get(1).is_some_and(|title| title.as_str().is_empty()))
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
    paste(text, at, tree, level, settings).map(|(outcome, message, _)| (outcome, message))
}

/// `org-yank` (`C-y`) with `org-yank-adjusted-subtrees`: a kill of one
/// or more subtrees, yanked at the start of a line or right after the
/// stars of an otherwise empty headline, goes in as `org-paste-subtree`
/// puts it (`for-yank`), the cursor on the line after it. None where Org
/// yanks the text as it is.
pub fn yank_subtree(
    text: &str,
    at: usize,
    kill: &str,
    settings: &Settings,
) -> Option<Result<(Outcome, String), Refusal>> {
    let at = at.min(text.len());
    let bol = line_start(text, at);
    let after_stars = !text[bol..at].is_empty()
        && text[bol..at].bytes().all(|b| b == b'*')
        && text[at..line_end(text, at)].bytes().all(|b| b == b' ' || b == b'\t');
    if !is_subtree(kill) || !(at == bol || after_stars) {
        return None;
    }
    Some(paste(text, at, kill, None, settings).map(|(mut outcome, message, end)| {
        // `org-yank` with `org-yank-folded-subtrees`: past the blank text
        // after it, at the start of that line.
        let skipped =
            end + (outcome.text.len() - end - outcome.text[end..].trim_start_matches([' ', '\t', '\n', '\r']).len());
        outcome.cursor = line_start(&outcome.text, skipped);
        (outcome, message)
    }))
}

/// `org-paste-subtree`, and where what it pasted ends.
fn paste(
    text: &str,
    at: usize,
    tree: &str,
    level: Option<usize>,
    settings: &Settings,
) -> Result<(Outcome, String, usize), Refusal> {
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
        // `org-outline-level`: stars with no space after them are no
        // headline, and the level is that of the headline they are under
        // (0 before the first).
        Some(level_at(text, bol).or_else(|| previous_heading(text, bol).and_then(|h| level_at(text, h))).unwrap_or(0))
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
    Ok((Outcome { text: buffer.text, cursor: first }, message, end))
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
