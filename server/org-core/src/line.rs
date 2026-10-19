//! Org's commands of a line: `org-beginning-of-line` (`C-a`) and
//! `org-end-of-line` (`C-e`), which stop at the headline's title and
//! before its tags, or after an item's bullet, as `org-special-ctrl-a/e`
//! says; `org-kill-line` (`C-k`), which kills to the tags first as
//! `org-special-ctrl-k` says.
//!
//! Everything is taken to be visible (the folds are the editor's), and
//! there is no `visual-line-mode`. Offsets are bytes.

use org_syntax::TodoKeywords;

use crate::list::{at_item, FULL_ITEM};
use crate::outline::{complex_heading_parts, HeadingParts};
use crate::text::tag_group;
use crate::todo::{column, tags_padding};

/// What `org-special-ctrl-a/e` says of one of the keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Special {
    /// The line's own boundary.
    #[default]
    Off,
    /// The special position first, the boundary on the next key.
    On,
    /// The boundary first, the special position on a key right after.
    Reversed,
}

fn line_start(text: &str, at: usize) -> usize {
    text[..at].rfind('\n').map_or(0, |index| index + 1)
}

fn line_end(text: &str, at: usize) -> usize {
    text[at..].find('\n').map_or(text.len(), |index| at + index)
}

/// `move-beginning-of-line` with the argument `count`: the start of the
/// line `count - 1` lines on, or as far as the text goes.
fn move_beginning_of_line(text: &str, at: usize, count: i64) -> usize {
    let mut start = line_start(text, at);
    if count > 1 {
        for _ in 1..count {
            let end = line_end(text, start);
            if end == text.len() {
                break;
            }
            start = end + 1;
        }
    } else {
        for _ in count..1 {
            if start == 0 {
                break;
            }
            start = line_start(text, start - 1);
        }
    }
    start
}

/// `(looking-at org-complex-heading-regexp)` at the start of a line: the
/// stars and the parts after them.
fn complex_heading(line: &str, todo: &TodoKeywords) -> Option<(usize, HeadingParts)> {
    let stars = line.bytes().take_while(|b| *b == b'*').count();
    if stars == 0 {
        return None;
    }
    let rest = &line[stars..];
    // A title follows a space; with none, only tags and blanks may.
    let only_tags =
        rest.trim_end_matches([' ', '\t']).is_empty() || tag_group(rest).is_some_and(|(blanks, _)| blanks == 0);
    if !rest.starts_with(' ') && !only_tags {
        return None;
    }
    Some((stars, complex_heading_parts(line, stars, todo)))
}

/// `org-beginning-of-line`: where `C-a` takes the cursor at `at`.
/// `repeated` says the key before was `C-a` too (`last-command`).
pub fn beginning_of_line(
    text: &str,
    at: usize,
    count: i64,
    special: Special,
    repeated: bool,
    todo: &TodoKeywords,
) -> usize {
    let origin = at;
    let bol = move_beginning_of_line(text, at, count);
    if special == Special::Off {
        return bol;
    }
    let eol = line_end(text, bol);
    let line = &text[bol..eol];
    // Where a key goes: the special position, or the line's start.
    let choose = |position: usize, first: bool| {
        if special == Special::Reversed {
            if origin == bol && repeated {
                position
            } else {
                bol
            }
        } else if first {
            position
        } else {
            bol
        }
    };
    if let Some((stars, parts)) = complex_heading(line, todo) {
        // Before the title, after any TODO keyword or priority cookie.
        let after = parts.priority.or(parts.keyword).map_or(stars, |(_, end)| end);
        let refpos = (bol + after + 1).min(eol);
        return choose(refpos, origin > refpos || origin <= bol);
    }
    if let Some(found) = FULL_ITEM.captures(line).filter(|_| at_item(text, bol)) {
        // After the bullet and the check-box, and a space after it.
        let after_bullet = match found.get(3) {
            None => found.get(1).map_or(0, |bullet| bullet.end()),
            Some(box_) if line[box_.end()..].starts_with(' ') => box_.end() + 1,
            Some(box_) => box_.end(),
        };
        let after_bullet = bol + after_bullet;
        return choose(after_bullet, origin > after_bullet || bol >= origin);
    }
    bol
}

/// `org-end-of-line`: where `C-e` takes the cursor at `at`. `repeated`
/// says the key before was `C-e` too.
pub fn end_of_line(text: &str, at: usize, count: i64, special: Special, repeated: bool, todo: &TodoKeywords) -> usize {
    let origin = at;
    let bol = move_beginning_of_line(text, at, count);
    let eol = line_end(text, bol);
    if special == Special::Off {
        return eol;
    }
    let line = &text[bol..eol];
    let Some((_, HeadingParts { tags: Some((tags, _)), .. })) = complex_heading(line, todo) else {
        return eol;
    };
    // Before the blanks before the tags.
    let tags = bol + line[..tags].trim_end_matches([' ', '\t']).len();
    if special == Special::Reversed {
        if origin == eol && repeated {
            tags
        } else {
            eol
        }
    } else if origin < tags || origin >= eol {
        tags
    } else {
        eol
    }
}

/// What `C-k` did: the text after, where the cursor is, and what was
/// killed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Killed {
    pub text: String,
    pub cursor: usize,
    pub kill: String,
}

/// How `org-kill-line` works: `org-special-ctrl-k`, and the tags aligned
/// after (`org-auto-align-tags`, `org-tags-column`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KillSettings {
    pub special: bool,
    pub auto_align_tags: bool,
    pub tags_column: i64,
}

/// `org-kill-line` at `at`.
pub fn kill_line(text: &str, at: usize, settings: KillSettings) -> Result<Killed, String> {
    let bol = line_start(text, at);
    let eol = line_end(text, at);
    let on_heading = |text: &str, bol: usize| {
        let stars = text[bol..].bytes().take_while(|b| *b == b'*').count();
        stars > 0 && text.as_bytes().get(bol + stars) == Some(&b' ')
    };
    let kill = |from: usize, to: usize| {
        let mut after = text.to_string();
        let kill = after[from..to].to_string();
        after.replace_range(from..to, "");
        (after, kill)
    };
    if !settings.special || at == bol || !on_heading(text, bol) {
        // `kill-line`: to the end of the line, or past it when only
        // blanks are left.
        if at == text.len() {
            return Err("End of buffer".to_string());
        }
        let end = if text[at..eol].trim_matches([' ', '\t']).is_empty() { (eol + 1).min(text.len()) } else { eol };
        let (text, kill) = kill(at, end);
        return Ok(Killed { text, cursor: at, kill });
    }
    let line = &text[bol..eol];
    // `org-tag-line-re`: tags after the stars' space or a blank.
    let tags =
        tag_group(line).filter(|(_, tags)| line[..*tags].len() > line.bytes().take_while(|b| *b == b'*').count());
    let Some((blanks, _)) = tags else {
        let (text, kill) = kill(at, eol);
        return Ok(Killed { text, cursor: at, kill });
    };
    let end = bol + blanks;
    let (mut after, kill) = if end <= at { kill(at, eol) } else { kill(at, end) };
    let bol = line_start(&after, at);
    let mut cursor = at;
    if settings.auto_align_tags && on_heading(&after, bol) {
        let eol = line_end(&after, bol);
        if let Some((blanks, tags, padding)) = tags_padding(&after[bol..eol], settings.tags_column) {
            let origin = at - bol;
            let wanted = column(&after[bol..at]);
            after.replace_range(bol + blanks..bol + tags, &padding);
            cursor = if origin > tags {
                at - (tags - blanks) + padding.len()
            } else if origin > blanks {
                // In the blanks, the cursor keeps its column.
                move_to_column(&after, bol, wanted)
            } else {
                at
            };
        }
    }
    Ok(Killed { text: after, cursor, kill })
}

/// `org-ctrl-k-protect-subtree`: whether `C-k` on a line whose end a
/// closed fold hides kills what it hides (`Off`), asks first (`Ask`), or
/// refuses (`Error`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Protect {
    #[default]
    Off,
    Ask,
    Error,
}

/// What `org-kill-line` asks before it kills a hidden subtree.
pub const KILL_HIDDEN_QUESTION: &str = "Kill hidden subtree along with headline? ";

/// What `org-kill-line` says when it kills no hidden subtree.
pub const KILL_HIDDEN_ABORTED: &str = "‘C-k’ aborted as it would kill a hidden subtree";

/// `org-kill-line` at `at` where a closed fold starts on the cursor's line
/// and hides the text up to `hidden_end`, the end of its last line: the
/// line the editor shows goes on to there. `answer` is the answer to the
/// question of `Protect::Ask`; `Ok(None)` when it is still to be asked.
/// Without a fold, or where `org-special-ctrl-k` kills only to the tags
/// or the end of the headline's own line, as `kill_line`.
pub fn kill_line_folded(
    text: &str,
    at: usize,
    settings: KillSettings,
    hidden_end: Option<usize>,
    protect: Protect,
    answer: Option<bool>,
) -> Result<Option<Killed>, String> {
    let bol = line_start(text, at);
    let on_heading = {
        let stars = text[bol..].bytes().take_while(|b| *b == b'*').count();
        stars > 0 && text.as_bytes().get(bol + stars) == Some(&b' ')
    };
    let Some(end) = hidden_end.filter(|end| *end > line_end(text, at)) else {
        return kill_line(text, at, settings).map(Some);
    };
    if settings.special && at != bol && on_heading {
        return kill_line(text, at, settings).map(Some);
    }
    match (protect, answer) {
        (Protect::Off, _) | (Protect::Ask, Some(true)) => {}
        (Protect::Ask, None) => return Ok(None),
        _ => return Err(KILL_HIDDEN_ABORTED.to_string()),
    }
    // `kill-line`: to the end of the visible line, past what the fold
    // hides; past its end too when only blanks are left before it.
    if at == text.len() {
        return Err("End of buffer".to_string());
    }
    let end = end.min(text.len());
    let blanks = text[at..end].len() - text[at..end].trim_start_matches([' ', '\t']).len();
    let end = if at + blanks == end { (end + 1).min(text.len()) } else { end };
    let mut after = text.to_string();
    let kill = after[at..end].to_string();
    after.replace_range(at..end, "");
    Ok(Some(Killed { text: after, cursor: at, kill }))
}

/// `move-to-column` on the line starting at `bol`.
fn move_to_column(text: &str, bol: usize, wanted: usize) -> usize {
    let line = &text[bol..line_end(text, bol)];
    let mut at = 0;
    for (index, c) in line.char_indices() {
        if column(&line[..index]) >= wanted {
            return bol + index;
        }
        at = index + c.len_utf8();
    }
    bol + at
}
